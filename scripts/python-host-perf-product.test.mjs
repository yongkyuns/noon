import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { readProductPerformanceAnchor } from "../.github/ci/product-performance-anchor.mjs";
import { productMeasurement } from "./playground-product-fps.mjs";
import { pairedCostConfidence, pairedThroughput, productPairOrder } from "./paired-product-metrics.mjs";
import { qualifyProductCohorts } from "./python-host-perf-product.mjs";

const { workloads } = await readProductPerformanceAnchor();
const directories = workloads.map(id => id === "parity-square-and-circle" ? "."
  : id === "showcase-camera-follows-path" ? "camera" : id);
const latencyNames = ["shell ready", "cold Run → applied", "warm Run → applied", "edit → applied"];
const noisy = [0.7, 1.3, 0.8, 1.2, 0.9, 1.1, 1];

function fixture(exampleId, { pairs = 7, fpsRatios = Array(pairs).fill(1),
  renderRatios = Array(pairs).fill(1) } = {}) {
  const measurement = productMeasurement(exampleId);
  const beforeFps = Array(pairs).fill(10), afterFps = fpsRatios.map(x => 10 * x);
  const beforeCost = Array(pairs).fill(1);
  return {
    exampleId,
    protocol: { pairs, order: Array.from({ length: pairs }, (_, i) => productPairOrder(i + 1)) },
    thresholds: { strictMinFpsRatio: 0.97 },
    measurements: Array.from({ length: pairs }, (_, i) => Object.fromEntries(
      ["baseline", "candidate"].map((side, j) => [side, {
        exampleId, label: side,
        pair: { index: i + 1, position: productPairOrder(i + 1).indexOf(side) + 1 },
        measurement,
        source: { path: measurement.sourcePath, sha256: "f".repeat(64) },
        fps: { effectiveFps: [beforeFps, afterFps][j][i] },
        rendererCosts: { cpuWallMs: { renderMs: { mean: [beforeCost, renderRatios][j][i] } } },
      }]))),
    fps: { paired: pairedThroughput(beforeFps, afterFps) },
    rendererCostQualification: measurement.gapClock === null ? null
      : { renderMs: pairedCostConfidence(beforeCost, renderRatios) },
    latency: Object.fromEntries(latencyNames.map(name => [name, { baselineMs: 100, candidateMs: 100 }])),
  };
}

async function exercise(change = () => {}, { bothScopes = false } = {}) {
  const cumulativeDirectories = directories.map(d => d === "." ? "cumulative" : `cumulative/${d}`);
  const reads = [];
  const result = await qualifyProductCohorts(async directory => {
    reads.push(directory);
    const cumulativeIndex = cumulativeDirectories.indexOf(directory);
    const cumulative = cumulativeIndex >= 0;
    const id = workloads[cumulative ? cumulativeIndex : directories.indexOf(directory)];
    assert.ok(id, `unexpected cohort directory ${directory}`);
    const input = fixture(id, { pairs: cumulative ? 3 : 7 });
    return cumulative && !bothScopes ? input : change(input, id) ?? input;
  });
  assert.deepEqual(reads, [...directories, ...cumulativeDirectories],
    "read every scope/cohort exactly once, in manifest order");
  assert.deepEqual(result.cohorts.map(c => c.exampleId), [...workloads, ...workloads]);
  return result;
}

test("all five declared workloads receive both strict qualifications", async () => {
  assert.equal(workloads.length, 5);
  const result = await exercise();
  assert.deepEqual(result.failures, []);
  assert.ok(result.cohorts.every(c => c.status === "pass"));
});

for (const id of workloads) {
  for (const [name, ratios, kind] of [["regression", Array(7).fill(0.96), "product_fps"],
    ["uncertainty", noisy, "product_fps_inconclusive"]]) {
    test(`${id} FPS ${name} blocks even when all other workloads pass`, async () => {
      const result = await exercise((input, example) => example === id ? fixture(id, { fpsRatios: ratios }) : input);
      assert.equal(result.failures.length, 1);
      assert.equal(result.failures[0].exampleId, id);
      assert.equal(result.failures[0].kind, kind);
      assert.equal(result.cohorts.filter(c => c.status === "pass").length, 2 * workloads.length - 1);
    });
  }
  test(`${id} preserves the 1.03 plus 20ms strict latency adjunct`, async () => {
    const result = await exercise((input, example) => {
      if (example === id) input.latency["shell ready"].candidateMs = 123.01;
      return input;
    });
    assert.equal(result.failures.length, 1);
    assert.equal(result.failures[0].kind, "product_latency");
    assert.equal(result.failures[0].exampleId, id);
  });
  test(`${id} missing evidence blocks without suppressing remaining cohorts`, async () => {
    const result = await exercise((input, example) => {
      if (example === id) throw new Error("missing retained comparison");
      return input;
    });
    assert.equal(result.failures.length, 1);
    assert.equal(result.failures[0].kind, "product_evidence");
    assert.equal(result.failures[0].exampleId, id);
  });
  if (productMeasurement(id).gapClock !== null) {
    for (const [name, ratios] of [["4% cost increase", Array(7).fill(1.04)], ["uncertainty", noisy]]) {
      test(`${id} render ${name} blocks despite a passing FPS improvement`, async () => {
        const result = await exercise((input, example) => example === id
          ? fixture(id, { fpsRatios: Array(7).fill(1.2), renderRatios: ratios }) : input);
        assert.equal(result.failures.length, 1);
        assert.equal(result.failures[0].kind, "product_render_cost");
        assert.equal(result.failures[0].exampleId, id);
      });
    }
  }
}

for (const [name, mutate] of [
  ["three-pair evidence", c => { c.protocol.pairs = 3; }],
  ["missing seventh pair", c => { c.measurements.pop(); }],
  ["wrong cohort", c => { c.exampleId = "parity-square-and-circle"; }],
  ["wrong report identity", c => { c.measurements[6].candidate.exampleId = "parity-square-and-circle"; }],
  ["stale protocol", c => { c.measurements[6].candidate.measurement = { gapClock: null }; }],
  ["missing source", c => { delete c.measurements[6].candidate.source; }],
  ["changed authored bytes", c => { c.measurements[6].candidate.source.sha256 = "e".repeat(64); }],
  ["missing latency", c => { delete c.latency["shell ready"]; }],
  ["invalid latency", c => { c.latency["shell ready"].candidateMs = NaN; }],
  ["changed threshold", c => { c.thresholds.strictMinFpsRatio = 0.8; }],
  ["invented FPS summary", c => { c.fps.paired.ratio = 2; }],
]) {
  test(`last added workload rejects ${name}`, async () => {
    const result = await exercise((input, id) => {
      if (id === workloads.at(-1)) mutate(input);
      return input;
    });
    assert.ok(result.failures.some(f => f.kind === "product_evidence" && f.exampleId === workloads.at(-1)));
  });
}

test("missing all evidence cannot produce an empty passing qualification", async () => {
  const result = await exercise(() => { throw new Error("missing"); }, { bothScopes: true });
  assert.equal(result.failures.length, 2 * workloads.length);
  assert.ok(result.cohorts.every(c => c.status === "blocked"));
});

test("production host harness consumes the manifest-driven audit after fixed work and before profiles", async () => {
  const runner = await readFile(new URL("./python-host-perf.mjs", import.meta.url), "utf8");
  const audit = runner.indexOf("const productQualification = await qualifyProductCohorts(");
  assert.ok(audit > runner.indexOf("rows.push({ workload, mode, sourceSha, pairs, costs })"));
  assert.ok(audit < runner.indexOf("// Profiles are a separate diagnostic experiment"));
  assert.match(runner, /failures\.push\(\.\.\.productQualification\.failures\)/);
  assert.match(runner, /"product-qualification\.json"/);
  assert.doesNotMatch(runner, /for \(const cohort of \["", "camera\/"\]\)/);
});
