import assert from "node:assert/strict";
import test from "node:test";
import { readProductPerformanceAnchor } from "../.github/ci/product-performance-anchor.mjs";
import { productMeasurement } from "./playground-product-fps.mjs";
import { pairedCostConfidence, pairedThroughput, productPairOrder } from "./paired-product-metrics.mjs";
import { qualifyProductCohorts } from "./python-host-perf-product.mjs";

const { workloads } = await readProductPerformanceAnchor();
const cases = ["adjacent", "cumulative"].flatMap(scope => workloads.map(exampleId => {
  const leaf = exampleId === "parity-square-and-circle" ? "."
    : exampleId === "showcase-camera-follows-path" ? "camera" : exampleId;
  return { scope, exampleId, pairs: scope === "adjacent" ? 7 : 3,
    directory: scope === "adjacent" ? leaf : leaf === "." ? "cumulative" : `cumulative/${leaf}` };
}));
const latencyNames = ["shell ready", "cold Run → applied", "warm Run → applied", "edit → applied"];

function comparison(spec, { fpsRatios, renderRatios } = {}) {
  const { exampleId, pairs: count } = spec;
  const measurement = productMeasurement(exampleId);
  const fpsBefore = Array(count).fill(10);
  const fpsAfter = (fpsRatios ?? Array(count).fill(1)).map(value => value * 10);
  const costBefore = Array(count).fill(1), costAfter = renderRatios ?? Array(count).fill(1);
  return {
    exampleId,
    protocol: { pairs: count, order: Array.from({ length: count }, (_, i) => productPairOrder(i + 1)) },
    thresholds: { strictMinFpsRatio: 0.97 },
    measurements: Array.from({ length: count }, (_, i) => Object.fromEntries(
      ["baseline", "candidate"].map((side, arm) => [side, {
        exampleId, label: side,
        pair: { index: i + 1, position: productPairOrder(i + 1).indexOf(side) + 1 },
        measurement, source: { path: measurement.sourcePath, sha256: "f".repeat(64) },
        fps: { effectiveFps: [fpsBefore, fpsAfter][arm][i] },
        rendererCosts: { cpuWallMs: { renderMs: { mean: [costBefore, costAfter][arm][i] } } },
      }]))),
    fps: { paired: pairedThroughput(fpsBefore, fpsAfter) },
    rendererCostQualification: measurement.gapClock === null ? null
      : { renderMs: pairedCostConfidence(costBefore, costAfter) },
    latency: Object.fromEntries(latencyNames.map(name => [name, { baselineMs: 100, candidateMs: 100 }])),
  };
}

async function exercise(change = input => input) {
  const reads = [];
  const result = await qualifyProductCohorts(async directory => {
    const spec = cases.find(row => row.directory === directory);
    assert.ok(spec, `unexpected comparison ${directory}`);
    reads.push(directory);
    return change(comparison(spec), spec);
  });
  return { result, reads };
}

test("strict qualification consumes all five seven-pair and all five three-pair cohorts once", async () => {
  const { result, reads } = await exercise();
  assert.deepEqual(reads, cases.map(c => c.directory));
  assert.deepEqual(result.failures, []);
  assert.equal(result.cohorts.length, 10);
  assert.deepEqual(result.cohorts.map(c => [c.scope, c.exampleId]),
    cases.map(c => [c.scope, c.exampleId]));
});

for (const exampleId of workloads) {
  for (const [name, fpsRatios, kind] of [
    ["4% FPS loss", [0.96, 0.96, 0.96], "product_fps"],
    ["uncertain FPS", [0.8, 1, 1.2], "product_fps_inconclusive"],
  ]) {
    test(`${exampleId}: cumulative ${name} blocks even with all adjacent metrics passing`, async () => {
      const { result, reads } = await exercise((input, spec) =>
        spec.scope === "cumulative" && spec.exampleId === exampleId
          ? comparison(spec, { fpsRatios }) : input);
      assert.equal(result.failures.length, 1);
      assert.equal(result.failures[0].scope, "cumulative");
      assert.equal(result.failures[0].exampleId, exampleId);
      assert.equal(result.failures[0].kind, kind);
      assert.equal(result.cohorts.filter(c => c.status === "pass").length, 9);
      assert.deepEqual(reads, cases.map(c => c.directory));
    });
  }
  test(`${exampleId}: cumulative strict latency is not replaced by the loose behavioral budget`, async () => {
    const { result } = await exercise((input, spec) => {
      if (spec.scope === "cumulative" && spec.exampleId === exampleId) {
        input.latency["shell ready"].candidateMs = 123.01;
      }
      return input;
    });
    assert.equal(result.failures.length, 1);
    assert.equal(result.failures[0].kind, "product_latency");
    assert.equal(result.failures[0].scope, "cumulative");
  });
  if (productMeasurement(exampleId).gapClock !== null) {
    for (const [name, renderRatios] of [["4% cost increase", [1.04, 1.04, 1.04]],
      ["cost uncertainty", [0.7, 1, 1.3]]]) {
      test(`${exampleId}: cumulative ${name} blocks despite faster FPS`, async () => {
        const { result } = await exercise((input, spec) =>
          spec.scope === "cumulative" && spec.exampleId === exampleId
            ? comparison(spec, { renderRatios, fpsRatios: [1.2, 1.2, 1.2] }) : input);
        assert.equal(result.failures.length, 1);
        assert.equal(result.failures[0].kind, "product_render_cost");
        assert.equal(result.failures[0].scope, "cumulative");
      });
    }
  }
}

for (const [name, mutate] of [
  ["missing last pair", c => c.measurements.pop()],
  ["invented FPS summary", c => { c.fps.paired.ratio = 2; }],
  ["reordered pair", c => c.protocol.order[1].reverse()],
  ["changed threshold", c => { c.thresholds.strictMinFpsRatio = 0.8; }],
  ["changed source", c => { c.measurements[2].candidate.source.sha256 = "e".repeat(64); }],
]) {
  test(`malformed cumulative evidence (${name}) blocks without suppressing other cohorts`, async () => {
    const { result, reads } = await exercise((input, spec) => {
      if (spec.scope === "cumulative" && spec.exampleId === workloads[0]) mutate(input);
      return input;
    });
    assert.equal(result.failures.length, 1);
    assert.equal(result.failures[0].kind, "product_evidence");
    assert.equal(result.failures[0].scope, "cumulative");
    assert.deepEqual(reads, cases.map(c => c.directory));
  });
}

test("missing every cumulative comparison cannot pass on adjacent evidence alone", async () => {
  const { result, reads } = await exercise((input, spec) => {
    if (spec.scope === "cumulative") throw new Error("missing anchor evidence");
    return input;
  });
  assert.equal(result.failures.length, 5);
  assert.ok(result.failures.every(f => f.scope === "cumulative" && f.kind === "product_evidence"));
  assert.deepEqual(reads, cases.map(c => c.directory));
});

for (const scope of ["adjacent", "cumulative"]) {
  test(`${scope} cannot substitute the other scope's pair count`, async () => {
    const { result } = await exercise((input, spec) => spec.scope === scope && spec.exampleId === workloads[0]
      ? comparison({ ...spec, pairs: spec.pairs === 7 ? 3 : 7 }) : input);
    assert.equal(result.failures.length, 1);
    assert.equal(result.failures[0].scope, scope);
    assert.equal(result.failures[0].kind, "product_evidence");
  });
}
