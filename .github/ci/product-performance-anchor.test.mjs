import assert from "node:assert/strict";
import { readFile, mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { productMeasurement } from "../../scripts/playground-product-fps.mjs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { validateProductPerformanceAnchor, readProductPerformanceAnchor } from "./product-performance-anchor.mjs";

test("repository pins a post-fix cumulative product source and existing workloads", async () => {
  const anchor = await readProductPerformanceAnchor();
  assert.equal(anchor.source, "58135c4069a6f8d700dcaaee677d7c8c7a48d6c5");
  assert.deepEqual(anchor.workloads,
    ["parity-square-and-circle", "showcase-camera-follows-path",
      "showcase-first-scene", "showcase-raster-images", "showcase-bezier-paths"]);
});

for (const [name, mutate, expected] of [
  ["schema", value => { value.schemaVersion = 2; }, /schema/],
  ["source", value => { value.source = "main"; }, /commit SHA/],
  ["label", value => { value.label = ""; }, /label/],
  ["policy", value => { value.policy = "best-of-three"; }, /alternating-pair policy/],
  ["workloads", value => { value.workloads = ["parity-square-and-circle"]; }, /workloads changed/],
]) {
  test(`invalid anchor ${name} fails closed`, () => {
    const value = {
      schemaVersion: 1,
      source: "a".repeat(40),
      label: "fixture",
      policy: "same-run-alternating-pairs",
      workloads: ["parity-square-and-circle", "showcase-camera-follows-path",
      "showcase-first-scene", "showcase-raster-images", "showcase-bezier-paths"],
    };
    mutate(value);
    assert.throws(() => validateProductPerformanceAnchor(value), expected);
  });
}

test("Product Gate builds, verifies, measures, and compares the pinned anchor", async () => {
  const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
  assert.match(workflow, /Pin cumulative performance anchor/);
  assert.match(workflow, /path: anchor/);
  assert.match(workflow, /Build cumulative anchor production package/);
  assert.match(workflow, /Verify cumulative anchor source, configuration and package contents/);
  assert.match(workflow, /Measure three alternating cumulative-anchor pairs/);
  assert.match(workflow, /noon_order="anchor candidate"/);
  assert.match(workflow, /noon_order="candidate anchor"/);
  assert.match(workflow, /product-performance-anchor.mjs cohorts/);
  assert.match(workflow, /noon_baseline=anchor/);
  assert.doesNotMatch(workflow, /best[- ]of|retry.*performance/i);
});

const root = fileURLToPath(new URL("../../", import.meta.url));
const cohortCommand = ".github/ci/product-performance-anchor.mjs";
const workloadIds = ["parity-square-and-circle", "showcase-camera-follows-path",
  "showcase-first-scene", "showcase-raster-images", "showcase-bezier-paths"];
const evidenceParts = [".", "camera", ...workloadIds.slice(2)];

function workflowShell(workflow, name) {
  const title = `      - name: ${name}\n`;
  assert.equal(workflow.split(title).length, 2, `one step named ${name}`);
  const step = workflow.split(title)[1].split("\n      - name:")[0];
  const code = step.split("        run: |\n")[1];
  assert.ok(code, `shell body for ${name}`);
  return code.trimEnd().split("\n").map(line => {
    assert.ok(line.startsWith("          "), "run block indentation");
    return line.slice(10);
  }).join("\n");
}

test("cohort CLI emits all five identities and stable, unique evidence directories", () => {
  const result = spawnSync(process.execPath, [cohortCommand, "cohorts"], { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(result.stdout.trim().split("\n").map(line => line.split("\t")),
    workloadIds.map((id, index) => [id, evidenceParts[index]]));
});

async function shellFixture(t) {
  const dir = await mkdtemp(path.join(os.tmpdir(), "noon-workload-wiring-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  await mkdir(path.join(dir, "bin"));
  const runner = path.join(dir, "fake-node.mjs");
  const log = path.join(dir, "calls.jsonl");
  // Execute the actual workflow shell and actual cohort CLI. Only browser and
  // comparison subprocesses are substituted to audit all invocations/failures.
  await writeFile(runner, `import { appendFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
const args = process.argv.slice(2), env = process.env;
if (args[0] === ${JSON.stringify(cohortCommand)}) {
  if (env.NOON_TEST_BAD_COHORTS === "1") process.exit(7);
  const result = spawnSync(process.execPath, args, { encoding: "utf8", env });
  process.stdout.write(result.stdout); process.stderr.write(result.stderr); process.exit(result.status ?? 1);
}
const call = { args, example: env.NOON_PRODUCT_EXAMPLE, label: env.NOON_PRODUCT_LABEL,
  site: env.NOON_PRODUCT_SITE_ROOT, pair: env.NOON_PRODUCT_PAIR_INDEX,
  position: env.NOON_PRODUCT_PAIR_POSITION, evidence: env.NOON_PRODUCT_ARTIFACT_DIR };
appendFileSync(env.NOON_TEST_LOG, JSON.stringify(call) + "\\n");
if (env.NOON_TEST_FAIL_MATCH && JSON.stringify(call).includes(env.NOON_TEST_FAIL_MATCH)) process.exit(3);
`);
  await writeFile(path.join(dir, "bin/node"),
    `#!/bin/sh\nexec ${JSON.stringify(process.execPath)} ${JSON.stringify(runner)} "$@"\n`, { mode: 0o755 });
  const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
  const run = (name, extra = {}) => spawnSync("bash", ["-e", "-c", workflowShell(workflow, name)], {
    cwd: root, encoding: "utf8", timeout: 20_000,
    env: { ...process.env, PATH: `${dir}/bin:${process.env.PATH}`, NOON_TEST_LOG: log,
      NOON_PRODUCT_WORKSPACE: dir, NOON_PRODUCT_ANCHOR_SHA: "58135c4069a6f8d700dcaaee677d7c8c7a48d6c5", ...extra },
  });
  const calls = async () => (await readFile(log, "utf8").catch(error => {
    if (error.code === "ENOENT") return "";
    throw error;
  })).trim().split("\n").filter(Boolean).map(line => JSON.parse(line));
  return { dir, run, calls };
}

test("actual measurement shell schedules 60 unique runs in both prescribed source cohorts", async t => {
  const fixture = await shellFixture(t);
  for (const name of ["Measure three alternating product pairs", "Measure three alternating cumulative-anchor pairs"]) {
    const result = fixture.run(name);
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
  }
  const calls = await fixture.calls();
  assert.equal(calls.length, 60);
  assert.equal(new Set(calls.map(call => path.normalize(call.evidence))).size, 60);
  let cursor = 0;
  for (const [scope, before] of [["", "baseline"], ["cumulative", "anchor"]]) {
    for (const [index, example] of workloadIds.entries()) {
      for (let pair = 1; pair <= 3; pair++) {
        const sides = pair === 2 ? ["candidate", before] : [before, "candidate"];
        for (const [position, side] of sides.entries()) {
          const call = calls[cursor++];
          assert.deepEqual(call.args, ["scripts/playground-product-e2e.mjs"]);
          assert.equal(call.example, example);
          assert.equal(call.site, path.join(fixture.dir, side));
          assert.equal(call.label, side === "anchor" ? "baseline" : side);
          assert.equal(call.pair, String(pair));
          assert.equal(call.position, String(position + 1));
          assert.equal(path.normalize(call.evidence), path.join("browser-smoke-artifacts/product-gate",
            scope, evidenceParts[index], side, `trial-${pair}`));
        }
      }
    }
  }
});

test("actual comparison shell retains all ten results and fails after a seeded cohort failure", async t => {
  const fixture = await shellFixture(t);
  const result = fixture.run("Compare product behavior", { NOON_TEST_FAIL_MATCH: "showcase-first-scene" });
  assert.ifError(result.error);
  assert.equal(result.status, 1, result.stderr);
  const calls = await fixture.calls();
  assert.equal(calls.length, 10);
  let cursor = 0;
  for (const [scope, before] of [["", "baseline"], ["cumulative", "anchor"]]) {
    for (const part of evidenceParts) {
      const [script, baseline, candidate, option, count] = calls[cursor++].args;
      assert.equal(script, "scripts/playground-product-compare.mjs");
      assert.deepEqual([option, count], ["--pairs", "3"]);
      assert.equal(path.normalize(baseline), path.join("browser-smoke-artifacts/product-gate", scope, part, before));
      assert.equal(path.normalize(candidate), path.join("browser-smoke-artifacts/product-gate", scope, part, "candidate"));
    }
  }
});

test("invalid cohort configuration fails the actual shell before any browser invocation", async t => {
  const fixture = await shellFixture(t);
  const result = fixture.run("Measure three alternating product pairs", { NOON_TEST_BAD_COHORTS: "1" });
  assert.ifError(result.error);
  assert.equal(result.status, 7);
  assert.deepEqual(await fixture.calls(), []);
});

test("a failed measured run stops its cohort instead of retrying or replacing it", async t => {
  const fixture = await shellFixture(t);
  const result = fixture.run("Measure three alternating cumulative-anchor pairs", {
    NOON_TEST_FAIL_MATCH: "showcase-first-scene",
  });
  assert.ifError(result.error);
  assert.equal(result.status, 3);
  const calls = await fixture.calls();
  assert.equal(calls.length, 13); // Two complete workloads, then the first failed mixed run.
  assert.equal(calls.at(-1).example, "showcase-first-scene");
  assert.equal(calls.at(-1).label, "baseline");
});


for (const exampleId of workloadIds) {
  test(`${exampleId} replaces an older gallery's source with the exact declared fixture`, async () => {
    const harness = await readFile(new URL("../../scripts/playground-product-e2e.mjs", import.meta.url), "utf8");
    const begin = harness.indexOf("  let authoredSource = ");
    const end = harness.indexOf("\n  const cold = ", begin);
    assert.ok(begin > 0 && end > begin, "fixture installation boundary");
    const install = new (Object.getPrototypeOf(async function() {}).constructor)(
      "readFile", "path", "repoRoot", "measurement", "exampleId", "page", "assert", "createHash",
      harness.slice(begin, end) + "\nreturn { source, authoredSource };",
    );
    const measurement = productMeasurement(exampleId);
    const original = await readFile(path.join(root, "web", measurement.sourcePath), "utf8");
    const editor = { value: "older or wrong source from the baseline gallery" };
    const page = { locator: selector => {
      assert.equal(selector, "#python-scene-source");
      return { evaluate: async (callback, value) => callback(editor, value) };
    } };
    const installed = await install(readFile, path, root, measurement, exampleId, page, assert, createHash);
    const expected = exampleId === "parity-square-and-circle"
      ? original.replace("self.play(Create(circle), Create(square))",
        "self.play(Create(circle), Create(square), run_time=4)\n        self.wait(0.5)")
      : original;
    assert.equal(editor.value, expected);
    assert.equal(installed.authoredSource, expected);
    assert.deepEqual(installed.source, { path: measurement.sourcePath,
      sha256: createHash("sha256").update(expected).digest("hex") });
  });
}
