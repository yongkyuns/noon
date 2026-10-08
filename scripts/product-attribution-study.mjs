// #1933: instrument the frozen baseline, not another acceptance attempt.
import assert from "node:assert/strict";
import { readFile, writeFile, appendFile, mkdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
const digest = b => createHash("sha256").update(b).digest("hex");
const git = (root, ...args) => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
const save = (p, v) => writeFile(p, JSON.stringify(v, null, 2) + "\n");

export function schedule(p) {
  assert.equal(p.schema, 1);
  assert.equal(p.studyId, "1933-scheduler-attribution-20261008-01");
  assert.equal(p.qualification, false);
  assert.equal(p.mergeApproval, false);
  assert.equal(p.build, "baseline");
  assert.equal(p.validatorSha, "35022a15f334e803aa57024490b25ea89872de05");
  assert.equal(p.validatorPlanSha256, "ffbe17eea5e9bfe20bd2761167bec612c547459139f1152572ac4d9e99a84ef9");
  assert.equal(p.harnessSha, "f5ec15c7a1a9a70c368e141abfc740f5081efc4e");
  assert.equal(p.harnessTree, "ecbfa794d50d61a4c3df3ef3111aa22e03998f0c");
  assert.deepEqual(p.workloads, ["parity-square-and-circle", "showcase-camera-follows-path"]);
  assert.deepEqual(p.pairs, [1, 2]);
  assert.equal(p.sampleIntervalMs, 100);
  assert.equal(p.pairDeadlineSeconds, 420);
  assert.equal(p.retries, 0);
  assert.equal(p.pairCommands, 4);
  assert.equal(p.observations, 8);
  return p.workloads.flatMap(workload => p.pairs.map(pair => ({
    workload, pair, build: "baseline", logicalOrder: pair % 2
      ? ["baseline", "candidate"] : ["candidate", "baseline"],
  })));
}

export async function run(planFile, validator, harness, baseline, output) {
  const bytes = await readFile(planFile), p = JSON.parse(bytes), events = schedule(p);
  assert.equal(process.env.GITHUB_RUN_ATTEMPT, "1", "no repeat of registered acquisition");
  assert.equal(process.env.GITHUB_EVENT_NAME, "pull_request");
  assert.equal((await readFile("/proc/sys/kernel/sched_schedstats", "utf8")).trim(), "1",
    "scheduler wait accounting is required; missing wait times are not zero");
  const event = JSON.parse(await readFile(process.env.GITHUB_EVENT_PATH, "utf8"));
  assert.equal(event.action, "opened");
  assert.equal(git(validator, "rev-parse", "HEAD"), p.validatorSha);
  assert.equal(git(harness, "rev-parse", "HEAD"), p.harnessSha);
  assert.equal(git(harness, "rev-parse", "HEAD^{tree}"), p.harnessTree);
  for (const root of [validator, harness, baseline]) git(root, "diff", "--exit-code", "HEAD", "--");
  const frozenBytes = await readFile(path.join(validator, "benchmarks/product-aa-study.json"));
  assert.equal(digest(frozenBytes), p.validatorPlanSha256);
  const helpers = await import(pathToFileURL(path.join(validator, "scripts/product-aa-study.mjs")));
  const original = helpers.validatePlan(JSON.parse(frozenBytes));
  assert.equal(process.version, `v${original.environment.node}`);
  assert.equal(git(baseline, "rev-parse", "HEAD"), original.sourcePair.baseline);
  const { verifyProductArtifact } = await import(pathToFileURL(path.join(harness, ".github/ci/product-artifact.mjs")));
  const { productMeasurement } = await import(pathToFileURL(path.join(harness, "scripts/playground-product-fps.mjs")));
  const env = { ...process.env, GITHUB_SHA: original.sourcePair.candidate,
    NOON_PRODUCT_CANDIDATE_SHA: original.sourcePair.candidate,
    NOON_PRODUCT_BASE_SHA: original.sourcePair.baseline, NOON_PRODUCT_HEAD_SHA: original.sourcePair.head,
    NOON_PRODUCT_EVENT_BASE_SHA: original.sourcePair.eventBase,
    NOON_WASM_PROFILE: "release", NOON_WASM_SKIP_OPT: "0", NOON_RENDERER_SMOKE: "0" };
  await helpers.exclusiveDirectory(output);
  const summary = { studyId: p.studyId, qualification: false, mergeApproval: false,
    instrumentation: true, acquisitionComplete: false, completedPairCommands: 0, errors: [] };
  try {
    const identity = await verifyProductArtifact(baseline, env, "baseline");
    await save(path.join(output, "registered-plan.json"), { plan: p, sha256: digest(bytes), events });
    await save(path.join(output, "original-identity.json"), identity);
    for (const workload of original.workloads) {
      const measurement = productMeasurement(workload);
      const raw = await readFile(path.join(baseline, "web", measurement.sourcePath), "utf8");
      assert.equal(helpers.authoredSourceFingerprint(workload, measurement, raw),
        original.authoredSourceHashes[workload]);
    }
    const probe = fileURLToPath(new URL("./product-scheduler-probe.py", import.meta.url));
    for (const item of events) {
      const dir = path.join(output, item.workload);
      await mkdir(dir, { recursive: true });
      await appendFile(path.join(output, "acquisition.jsonl"), JSON.stringify({ ...item, phase: "start", at: Date.now() }) + "\n");
      await helpers.runRecorded("python3", [probe, "--output", path.join(dir, `scheduler-${item.pair}.jsonl`),
        "--interval-ms", String(p.sampleIntervalMs), "--deadline-seconds", String(p.pairDeadlineSeconds),
        "--", process.execPath, path.join(harness, original.pairRunner)], {
        cwd: harness, env: helpers.pairEnvironment(original, item, baseline, dir, process.env),
        log: path.join(dir, `pair-${item.pair}.log`), timeoutMs: 450_000, graceMs: 5_000,
      });
      const reports = {};
      for (const side of ["baseline", "candidate"]) {
        reports[side] = JSON.parse(await readFile(path.join(dir, side, `trial-${item.pair}`, "report.json")));
      }
      helpers.validateReports(original, item, reports, productMeasurement(item.workload),
        original.authoredSourceHashes[item.workload]);
      summary.completedPairCommands++;
      await appendFile(path.join(output, "acquisition.jsonl"), JSON.stringify({ ...item, phase: "complete", at: Date.now() }) + "\n");
    }
    assert.deepEqual(await verifyProductArtifact(baseline, env, "baseline"), identity);
    for (const root of [validator, harness, baseline]) git(root, "diff", "--exit-code", "HEAD", "--");
    summary.acquisitionComplete = true;
  } catch (error) {
    summary.errors.push(error.stack); throw error;
  } finally {
    await save(path.join(output, "summary.json"), summary);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2).map(x => path.resolve(x));
  assert.equal(args.length, 5, "plan validator harness baseline output");
  await run(...args);
}
