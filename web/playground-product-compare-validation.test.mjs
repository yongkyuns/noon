import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

const command = new URL("../scripts/playground-product-compare.mjs", import.meta.url);
const pairCountArgs = ["--pairs", "3"];
const report = () => ({
  schemaVersion: 2,
  exampleId: "validation-fixture",
  runtime: { backend: "webgpu" },
  shellReadyMs: 100,
  coldRunMs: 200,
  warmRunMs: 50,
  editRunMs: 60,
  fps: { effectiveFps: 60 },
  measurement: { version: 1, clock: "renderer-sampled", preparation: "completed-cold-pass",
    authoredSeconds: 4, warmupSeconds: 1, endpointHoldSeconds: 0.5 },
  runtimeIdentity: { sourceRevision: "a".repeat(40), buildId: "b".repeat(64) },
  screenshot: "intentionally-not-loaded.png",
});

// Exercise the real CLI. Malformed measurements must fail before PNG loading,
// so these input-contract tests need neither a renderer nor image dependencies.
async function rejectsReport(baseline, candidate, expected, overrides = {}) {
  const directory = await mkdtemp(path.join(tmpdir(), "noon-product-input-"));
  try {
    const directories = [path.join(directory, "baseline"), path.join(directory, "candidate")];
    for (const [index, input] of [baseline, candidate].entries()) {
      await mkdir(directories[index]);
      await writeFile(path.join(directories[index], "report.json"), JSON.stringify(input));
    }
    const result = spawnSync(process.execPath, [command.pathname, ...directories], {
      encoding: "utf8", timeout: 10_000, env: cleanEnvironment(overrides),
    });
    assert.ifError(result.error);
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, expected);
    assert.doesNotMatch(result.stderr, /ERR_MODULE_NOT_FOUND|ENOENT/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

function cleanEnvironment(overrides = {}) {
  const env = { ...process.env };
  for (const key of Object.keys(env)) {
    if (key.startsWith("NOON_PRODUCT_")) delete env[key];
  }
  return { ...env, ...overrides };
}

async function writeScreenshot(directory, color) {
  const { default: pngjs } = await import("pngjs");
  const { PNG } = pngjs;
  const png = new PNG({ width: 2, height: 2 });
  for (let offset = 0; offset < png.data.length; offset += 4) {
    png.data.set([...color, 255], offset);
  }
  await writeFile(path.join(directory, "frame.png"), PNG.sync.write(png));
}

function cohortReport(side, index, position, startedAtMs, fps = 60) {
  return {
    ...report(),
    label: side,
    pair: { index, position },
    startedAtMs,
    finishedAtMs: startedAtMs + 500,
    fps: { effectiveFps: fps },
    runtimeIdentity: side === "baseline"
      ? { sourceRevision: "a".repeat(40), buildId: "b".repeat(64) }
      : { sourceRevision: "c".repeat(40), buildId: "d".repeat(64) },
    screenshot: "frame.png",
  };
}

async function createCohort(root, {
  baselineFps = [59, 62, 60],
  candidateFps = [59, 61, 63],
  visualRegressionPair = null,
  mutateReport = () => {},
} = {}) {
  const sequence = [
    [1, "baseline"], [1, "candidate"],
    [2, "candidate"], [2, "baseline"],
    [3, "baseline"], [3, "candidate"],
  ];
  const schedule = new Map(sequence.map((item, index) => [item.join(":"), index]));
  for (let index = 1; index <= 3; index += 1) {
    for (const side of ["baseline", "candidate"]) {
      const trialDirectory = path.join(root, side, `trial-${index}`);
      await mkdir(trialDirectory, { recursive: true });
      const position = index === 2
        ? (side === "candidate" ? 1 : 2)
        : (side === "baseline" ? 1 : 2);
      const scheduleIndex = schedule.get(`${index}:${side}`);
      const value = side === "baseline" ? baselineFps[index - 1] : candidateFps[index - 1];
      const input = cohortReport(side, index, position, scheduleIndex * 1_000, value);
      mutateReport(input, { side, index });
      await writeFile(path.join(trialDirectory, "report.json"), JSON.stringify(input));
      const color = visualRegressionPair === index && side === "candidate"
        ? [255, 0, 0] : [0, 0, 0];
      await writeScreenshot(trialDirectory, color);
    }
  }
}

async function runCohort(root, overrides = {}) {
  return spawnSync(process.execPath, [command.pathname,
    path.join(root, "baseline"), path.join(root, "candidate"), ...pairCountArgs], {
    encoding: "utf8", timeout: 10_000, env: cleanEnvironment(overrides),
  });
}

async function withTempDirectory(prefix, callback) {
  const directory = await mkdtemp(path.join(tmpdir(), prefix));
  try {
    await callback(directory);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

test("three noisy alternating pairs pass and retain every report and dispersion", async () => {
  await withTempDirectory("noon-product-cohort-pass-", async directory => {
    await createCohort(directory);
    const result = await runCohort(directory);
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    const comparison = JSON.parse(await readFile(path.join(directory, "candidate", "comparison.json")));
    assert.deepEqual(comparison.protocol.order,
      [["baseline", "candidate"], ["candidate", "baseline"], ["baseline", "candidate"]]);
    assert.equal(comparison.measurements.length, 3);
    assert.deepEqual(comparison.measurements.map(pair => [pair.baseline.label, pair.candidate.label]),
      Array.from({ length: 3 }, () => ["baseline", "candidate"]));
    assert.equal(comparison.statistics.baseline.fps.min, 59);
    assert.equal(comparison.statistics.baseline.fps.max, 62);
    assert.notEqual(comparison.statistics.baseline.fps.mean, comparison.statistics.candidate.fps.mean);
  });
});

test("a seeded FPS regression across all candidate runs fails the unchanged floor", async () => {
  await withTempDirectory("noon-product-cohort-fps-", async directory => {
    await createCohort(directory, { baselineFps: [60, 61, 59], candidateFps: [42, 44, 43] });
    const result = await runCohort(directory);
    assert.ifError(result.error);
    assert.equal(result.status, 2);
    assert.match(result.stderr, /effective FPS regressed/);
    const comparison = JSON.parse(await readFile(path.join(directory, "candidate", "comparison.json")));
    assert.equal(comparison.fps.floor, 48);
    assert.equal(comparison.fps.candidate, 43);
  });
});

for (const [name, mutateReport, removeReport, expected] of [
  ["duplicate pair position", (input, { side, index }) => {
    if (side === "candidate" && index === 2) input.pair.position = 2;
  }, null, /declared alternating order/],
  ["reordered pair", (input, { side, index }) => {
    if (side === "baseline" && index === 3) input.pair.position = 2;
  }, null, /declared alternating order/],
  ["changed package identity", (input, { side, index }) => {
    if (side === "candidate" && index === 2) input.runtimeIdentity.buildId = "e".repeat(64);
  }, null, /product package changed between trials/],
  ["overlapping run intervals", (input, { side, index }) => {
    if (side === "candidate" && index === 1) input.startedAtMs = 100;
  }, null, /paired runs must be serial/],
  ["changed browser configuration", (input, { side, index }) => {
    if (side === "candidate" && index === 2) input.runtime.deviceScaleFactor = 2;
  }, null, /browser\/runtime configuration changed/],
  ["changed warmup protocol", (input, { side, index }) => {
    if (side === "baseline" && index === 2) input.measurement.warmupSeconds = 0;
  }, null, /product measurement protocol changed/],
  ["missing trial report", () => {}, (root) => path.join(root, "candidate", "trial-2", "report.json"), /ENOENT|no such file/i],
]) {
  test(`${name} is rejected before image decoding`, async () => {
    await withTempDirectory("noon-product-cohort-invalid-", async directory => {
      await createCohort(directory, { mutateReport });
      const missing = removeReport?.(directory);
      if (missing) await rm(missing);
      const result = await runCohort(directory);
      assert.ifError(result.error);
      assert.equal(result.status, 1, result.stderr);
      assert.match(result.stderr, expected);
      assert.doesNotMatch(result.stderr, /ERR_MODULE_NOT_FOUND|pngjs/);
    });
  });
}

test("a visual regression in pair two fails even when pair one matches", async () => {
  await withTempDirectory("noon-product-cohort-visual-", async directory => {
    await createCohort(directory, { visualRegressionPair: 2 });
    const result = await runCohort(directory);
    assert.ifError(result.error);
    assert.equal(result.status, 2);
    assert.match(result.stderr, /pair 2 deterministic frame visual diff/);
    const comparison = JSON.parse(await readFile(path.join(directory, "candidate", "comparison.json")));
    assert.equal(comparison.visual.pairs[0].diffRatio, 0);
    assert.equal(comparison.visual.pairs[1].diffRatio, 1);
  });
});

for (const side of ["baseline", "candidate"]) {
  for (const value of [undefined, null, "", "60", false, 0, -1]) {
    test(`${side} FPS rejects ${String(value)} (${typeof value})`, async () => {
      const inputs = { baseline: report(), candidate: report() };
      inputs[side].fps.effectiveFps = value;
      await rejectsReport(inputs.baseline, inputs.candidate, /effective FPS must be a finite positive number/);
    });
  }
  for (const key of ["shellReadyMs", "coldRunMs", "warmRunMs", "editRunMs"]) {
    test(`${side} ${key} rejects negative latency`, async () => {
      const inputs = { baseline: report(), candidate: report() };
      inputs[side][key] = -1;
      await rejectsReport(inputs.baseline, inputs.candidate, /latency must be finite and non-negative/);
    });
  }
}

for (const [key, values] of Object.entries({
  NOON_PRODUCT_MAX_LATENCY_RATIO: ["", "NaN", "Infinity", "0", "-1"],
  NOON_PRODUCT_LATENCY_SLACK_MS: [" ", "NaN", "Infinity", "-1"],
  NOON_PRODUCT_MIN_FPS_RATIO: ["", "NaN", "Infinity", "0", "-1"],
  NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO: ["", "NaN", "Infinity", "-1", "1.01"],
})) {
  for (const value of values) {
    test(`${key} rejects ${value}`, async () => {
      await rejectsReport(report(), report(), new RegExp(key), { [key]: value });
    });
  }
}

for (const [name, mutate, expected] of [
  ["missing schema", (baseline) => { delete baseline.schemaVersion; }, /baseline product report has an unsupported schema/],
  ["missing example", (baseline) => { baseline.exampleId = null; }, /baseline product report must name an example/],
  ["missing backend", (_baseline, candidate) => { candidate.runtime = {}; }, /candidate product report must name a renderer backend/],
]) {
  test(name, async () => {
    const baseline = report();
    const candidate = report();
    mutate(baseline, candidate);
    await rejectsReport(baseline, candidate, expected);
  });
}
