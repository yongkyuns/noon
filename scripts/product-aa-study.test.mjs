import assert from "node:assert/strict";
import { readFile, mkdtemp, rm, mkdir } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  validatePlan, studySchedule, pairEnvironment, validateReports, strictLatencyFailures,
  exclusiveDirectory, runRecorded,
} from "./product-aa-study.mjs";

const plan = JSON.parse(await readFile(new URL("../benchmarks/product-aa-study.json", import.meta.url)));
const clone = value => structuredClone(value);

test("the registered schedule has exactly 100 pair commands and 200 observations", () => {
  const runs = studySchedule(plan);
  assert.equal(runs.length, 100);
  assert.equal(runs.flatMap(r => r.logicalOrder).length, 200);
  assert.deepEqual(runs.map(r => r.ordinal), Array.from({ length: 100 }, (_, i) => i + 1));
  assert.equal(new Set(runs.map(r => [r.cohort, r.build, r.workload, r.pair].join("/"))).size, 100);
  for (const cohort of plan.cohorts) for (const build of plan.builds) for (const workload of plan.workloads) {
    const group = runs.filter(r => r.cohort === cohort.name && r.build === build.name && r.workload === workload);
    assert.deepEqual(group.map(r => r.pair), Array.from({ length: cohort.pairs }, (_, i) => i + 1));
    for (const r of group) {
      assert.deepEqual(r.logicalOrder, r.pair % 2 ? ["baseline", "candidate"] : ["candidate", "baseline"]);
    }
  }
  for (let i = 0; i < runs.length; i += 2) {
    assert.equal(runs[i].pair, runs[i + 1].pair);
    assert.equal(runs[i].workload, runs[i + 1].workload);
    assert.deepEqual([runs[i].build, runs[i + 1].build],
      runs[i].pair % 2 ? ["baseline", "candidate"] : ["candidate", "baseline"]);
  }
});

for (const [name, mutate] of [
  ["missing workload", p => p.workloads.pop()],
  ["extra workload", p => p.workloads.push("another")],
  ["changed order", p => p.workloads.reverse()],
  ["smaller cohort", p => p.cohorts[1].pairs = 2],
  ["extra repetitions", p => p.retries = 1],
  ["qualified label", p => p.qualification = true],
  ["resume after failure", p => p.stopOnTrialError = false],
  ["wrong original source", p => p.builds[1].source = "a".repeat(40)],
  ["duplicate artifact", p => p.builds[1].artifactId = p.builds[0].artifactId],
  ["invalid package checksum", p => p.builds[0].zipSha256 = "missing"],
  ["unknown browser revision", p => p.environment.browserVersion = "changed"],
  ["manual rerun", p => p.execution.allowedRunAttempt = 2],
  ["unregistered event", p => p.execution.event = "workflow_dispatch"],
  ["different measurement script", p => p.pairRunner = "changed.mjs"],
]) {
  test(`rejects ${name}`, () => {
    const p = clone(plan); mutate(p); assert.throws(() => validatePlan(p));
  });
}

test("each A/A pair serves one identical original root despite contaminated inherited variables", () => {
  for (const event of studySchedule(plan)) {
    const root = `/sources/${event.build}`;
    const env = pairEnvironment(plan, event, root, "/evidence", {
      PATH: "/usr/bin", NOON_PRODUCT_BROWSER_WS_ENDPOINT: "wrong",
      NOON_PRODUCT_SITE_ROOT: "/wrong", NOON_PRODUCT_MIN_FPS_RATIO: "0.1", NOON_PERF_RERUN: "1",
    });
    assert.equal(env.NOON_PRODUCT_REFERENCE_ROOT, root);
    assert.equal(env.NOON_PRODUCT_CANDIDATE_ROOT, root);
    assert.equal(env.NOON_PRODUCT_PAIR_INDEX, String(event.pair));
    assert.equal(env.NOON_PRODUCT_EXAMPLE, event.workload);
    assert.equal(env.NOON_PRODUCT_BROWSER_WS_ENDPOINT, undefined);
    assert.equal(env.NOON_PRODUCT_SITE_ROOT, undefined);
    assert.equal(env.NOON_PERF_RERUN, undefined);
    assert.equal(env.NOON_PRODUCT_STRICT_MIN_FPS_RATIO, "0.97");
    assert.equal(env.NOON_PRODUCT_MIN_FPS_RATIO, "0.80");
    assert.equal(env.PATH, "/usr/bin");
  }
});

function fixture() {
  const event = studySchedule(plan)[0];
  const measurement = { version: 5, sourcePath: "python/example.py", gapClock: null };
  const sourceHash = "c".repeat(64);
  const reports = {};
  for (const [position, side] of event.logicalOrder.entries()) {
    reports[side] = {
      label: side, exampleId: event.workload, pair: { index: 1, position: position + 1 },
      measurement: clone(measurement), source: { path: measurement.sourcePath, sha256: sourceHash },
      runtimeIdentity: { sourceRevision: plan.builds[0].source, buildId: plan.builds[0].buildId,
        files: { wasm: { sha256: "d".repeat(64) } } },
      runtime: { browserVersion: plan.environment.browserVersion, backend: plan.environment.renderer,
        gpuMode: plan.environment.browserMode, sharedBrowserProcess: true },
      fps: { effectiveFps: 50 },
    };
  }
  return { event, measurement, sourceHash, reports };
}

test("same-package, same-source original report identity is accepted", () => {
  const f = fixture();
  validateReports(plan, f.event, f.reports, f.measurement, f.sourceHash);
});
for (const [name, mutate] of [
  ["missing observation", r => delete r.candidate],
  ["different build", r => r.candidate.runtimeIdentity.buildId = plan.builds[1].buildId],
  ["different source revision", r => r.candidate.runtimeIdentity.sourceRevision = "e".repeat(40)],
  ["different WASM identity", r => r.candidate.runtimeIdentity.files.wasm.sha256 = "f".repeat(64)],
  ["modified authored source", r => r.candidate.source.sha256 = "f".repeat(64)],
  ["altered measurement window", r => r.candidate.measurement.windowEndSeconds = 999],
  ["wrong pair position", r => r.candidate.pair.position = 1],
  ["wrong trial index", r => r.candidate.pair.index = 2],
  ["wrong browser", r => r.candidate.runtime.browserVersion = "newer"],
  ["hardware backend substitution", r => r.candidate.runtime.backend = "WebGPU"],
  ["separate browser processes", r => r.candidate.runtime.sharedBrowserProcess = false],
  ["invalid FPS", r => r.candidate.fps.effectiveFps = NaN],
]) {
  test(`retains a blocking error for ${name}`, () => {
    const f = fixture(); mutate(f.reports);
    assert.throws(() => validateReports(plan, f.event, f.reports, f.measurement, f.sourceHash));
  });
}

test("strict latency retains the original 1.03 ratio plus 20ms; a slow trial is not dropped", () => {
  const latency = Object.fromEntries(
    ["shell ready", "cold Run → applied", "warm Run → applied", "edit → applied"]
      .map(name => [name, { baselineMs: 100, candidateMs: 123 }]));
  assert.deepEqual(strictLatencyFailures({ latency }), []);
  latency["shell ready"].candidateMs = 123.01;
  assert.equal(strictLatencyFailures({ latency }).length, 1);
  delete latency["edit → applied"];
  assert.throws(() => strictLatencyFailures({ latency }));
});

async function directory(t) {
  const d = await mkdtemp(path.join(os.tmpdir(), "noon-aa-test-"));
  t.after(() => rm(d, { recursive: true, force: true }));
  return d;
}

test("a new output directory cannot overwrite a previous or partial study", async t => {
  const d = await directory(t), output = path.join(d, "evidence");
  await exclusiveDirectory(output);
  await assert.rejects(() => exclusiveDirectory(output), { code: "EEXIST" });
});

test("recorded successful command retains its stdout and stderr", async t => {
  const d = await directory(t), log = path.join(d, "success.log");
  const result = await runRecorded(process.execPath,
    ["-e", "console.log('output');console.error('diagnostic')"], { cwd: d, env: process.env, log });
  assert.equal(result.code, 0);
  assert.equal(result.timedOut, false);
  const text = await readFile(log, "utf8");
  assert.match(text, /output/); assert.match(text, /diagnostic/);
  await assert.rejects(() => runRecorded(process.execPath, ["-e", ""],
    { cwd: d, env: process.env, log }), { code: "EEXIST" });
});

test("a failed command is recorded once, rejected, and never retried", async t => {
  const d = await directory(t), log = path.join(d, "failure.log");
  await assert.rejects(() => runRecorded(process.execPath,
    ["-e", "console.log('one attempt');process.exit(3)"], { cwd: d, env: process.env, log }),
    error => error.result.code === 3 && error.result.timedOut === false);
  assert.equal((await readFile(log, "utf8")).trim(), "one attempt");
});

test("the acquisition watchdog kills a hanging child and rejects its partial trial", async t => {
  const d = await directory(t), log = path.join(d, "timeout.log");
  await assert.rejects(() => runRecorded(process.execPath, ["-e", "setInterval(()=>{},1000)"],
    { cwd: d, env: process.env, log, timeoutMs: 100, graceMs: 100 }),
    error => error.result.timedOut);
});

test("a comparator budget failure is evidence, never a source of replacement measurements", async t => {
  const d = await directory(t), log = path.join(d, "classification.log");
  const result = await runRecorded(process.execPath, ["-e", "process.exit(2)"],
    { cwd: d, env: process.env, log, allowedCodes: [0, 2] });
  assert.equal(result.code, 2);
});

test("the diagnostic workflow opens only once and preserves artifacts even on failure", async () => {
  const workflow = await readFile(new URL("../.github/workflows/perf-product-aa.yml", import.meta.url), "utf8");
  assert.match(workflow, /types: \[opened\]/);
  assert.doesNotMatch(workflow, /workflow_dispatch:|synchronize|schedule:/);
  assert.match(workflow, /cancel-in-progress: false/);
  assert.match(workflow, /actions: read/);
  assert.doesNotMatch(workflow, /(?:actions|contents|pull-requests): write/);
  assert.match(workflow, /GITHUB_RUN_ATTEMPT.*= 1/);
  assert.match(workflow, /if: always\(\)/);
  assert.match(workflow, /if-no-files-found: error/);
  assert.match(workflow, /retention-days: 30/);
  assert.match(workflow, /sha256/);
  assert.doesNotMatch(workflow, /cargo (?:build|test)|wasm-pack build|rerun/);
  assert.match(workflow, /node control\/scripts\/product-aa-study.mjs/);
});

test("controller never edits the original product harness or uses its own confidence estimator", async () => {
  const source = await readFile(new URL("./product-aa-study.mjs", import.meta.url), "utf8");
  assert.match(source, /qualifyProductMetrics\(comparison, cohort\.pairs\)/);
  assert.match(source, /plan\.pairRunner/);
  assert.match(source, /plan\.comparator/);
  assert.match(source, /summary\.acquisitionComplete = true;[\s\S]*for \(const cohort/);
  assert.doesNotMatch(source, /Math\.log|Student|t95|before\.sort|after\.sort|slice\(1\)/);
  assert.match(source, /qualification: false, mergeApproval: false/);
});
