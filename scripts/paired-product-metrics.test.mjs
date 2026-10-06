import assert from "node:assert/strict";
import test from "node:test";
import { pairedCostConfidence, pairedThroughput, pairedLogInterval,
  PRODUCT_PAIR_COUNT, productPairOrder, readProductPair, qualifyProductMetrics } from "./paired-product-metrics.mjs";

test("three noisy camera FPS pairs remain inconclusive instead of becoming a regression", () => {
  const result = pairedThroughput(
    [4.638487852959982, 5.156706583395439, 5.7379277021108726],
    [4.291722697490801, 5.153753650575537, 4.449718184514981],
  );
  assert.equal(result.status, "inconclusive");
  assert.ok(result.lower < 0.97 && result.upper > 0.97);
});

test("seven stable throughput pairs distinguish pass from demonstrated regression", () => {
  const baseline = [10, 10.1, 9.9, 10.2, 9.8, 10.05, 9.95];
  assert.equal(pairedThroughput(baseline, baseline.map(value => value * 0.995)).status, "pass");
  assert.equal(pairedThroughput(baseline, baseline.map(value => value * 0.90)).status, "regression");
});

test("renderer CPU cost only regresses when the paired interval is clearly above the budget", () => {
  const baseline = [1, 1.02, 0.98, 1.01, 0.99, 1.03, 0.97];
  assert.equal(pairedCostConfidence(baseline, baseline.map(value => value * 0.95)).status, "pass");
  assert.equal(pairedCostConfidence(baseline, baseline.map(value => value * 1.08)).status, "regression");
});

test("wide cost uncertainty stays inconclusive", () => {
  const baseline = [1, 1, 1, 1, 1, 1, 1];
  const candidate = [0.7, 1.35, 0.8, 1.25, 0.75, 1.3, 0.85];
  assert.equal(pairedCostConfidence(baseline, candidate).status, "inconclusive");
});


test("the actual producer admits every prescribed pair and reverses all even pairs", () => {
  assert.equal(PRODUCT_PAIR_COUNT, 7);
  assert.equal(readProductPair({}), null);
  for (let index = 1; index <= 7; index++) {
    const expected = index % 2 === 0 ? ["candidate", "baseline"] : ["baseline", "candidate"];
    assert.deepEqual(productPairOrder(index), expected);
    for (const [i, label] of expected.entries()) {
      assert.deepEqual(readProductPair({ NOON_PRODUCT_PAIR_INDEX: String(index),
        NOON_PRODUCT_PAIR_POSITION: String(i + 1), NOON_PRODUCT_LABEL: label }),
      { index, position: i + 1 });
    }
  }
});

test("producer rejects missing, out-of-range and reordered pair metadata", () => {
  const valid = { NOON_PRODUCT_PAIR_INDEX: "4", NOON_PRODUCT_PAIR_POSITION: "1",
    NOON_PRODUCT_LABEL: "candidate" };
  for (const index of [undefined, "", "0", "8", "4.0", " 4", "NaN"]) {
    assert.throws(() => readProductPair({ ...valid, NOON_PRODUCT_PAIR_INDEX: index }), /pair index/);
  }
  for (const position of [undefined, "", "0", "3", "1.0"]) {
    assert.throws(() => readProductPair({ ...valid, NOON_PRODUCT_PAIR_POSITION: position }), /pair position/);
  }
  assert.throws(() => readProductPair({ ...valid, NOON_PRODUCT_LABEL: "baseline" }), /alternating order/);
});

function metricComparison({ fpsRatios = Array(7).fill(1), renderRatios = Array(7).fill(1) } = {}) {
  const beforeFps = Array(7).fill(10), afterFps = fpsRatios.map(ratio => ratio * 10);
  const beforeCost = Array(7).fill(1), afterCost = renderRatios;
  const measurement = { gapClock: "renderer-submission" };
  return {
    protocol: { pairs: 7, order: Array.from({ length: 7 }, (_, i) => productPairOrder(i + 1)) },
    thresholds: { strictMinFpsRatio: 0.97 },
    measurements: Array.from({ length: 7 }, (_, i) => Object.fromEntries(
      ["baseline", "candidate"].map((side, j) => [side, {
        label: side, pair: { index: i + 1, position: productPairOrder(i + 1).indexOf(side) + 1 },
        measurement, fps: { effectiveFps: [beforeFps, afterFps][j][i] },
        rendererCosts: { cpuWallMs: { renderMs: { mean: [beforeCost, afterCost][j][i] } } },
      }]))),
    fps: { paired: pairedThroughput(beforeFps, afterFps) },
    rendererCostQualification: { renderMs: pairedCostConfidence(beforeCost, afterCost) },
  };
}

const noisy = [0.7, 1.3, 0.8, 1.2, 0.9, 1.1, 1.0];
for (const [fpsStatus, fpsRatios] of [["pass", Array(7).fill(1)],
  ["inconclusive", noisy], ["regression", Array(7).fill(0.9)]]) {
  for (const [renderStatus, renderRatios] of [["pass", Array(7).fill(1)],
    ["inconclusive", noisy], ["regression", Array(7).fill(1.08)]]) {
    test(`FPS ${fpsStatus} / render ${renderStatus} cannot erase a failed or inconclusive metric`, () => {
      const actual = qualifyProductMetrics(metricComparison({ fpsRatios, renderRatios }));
      assert.equal(actual.fps.status, fpsStatus);
      assert.equal(actual.render.status, renderStatus);
      const statuses = [fpsStatus, renderStatus];
      assert.equal(actual.status, statuses.includes("regression") ? "regression"
        : statuses.includes("inconclusive") ? "inconclusive" : "pass");
    });
  }
}

test("FPS uncertainty remains inconclusive even with a favorable point estimate or no render metric", () => {
  const c = metricComparison({ fpsRatios: noisy.map(x => x * 1.04) });
  c.measurements.forEach(pair => {
    for (const side of ["baseline", "candidate"]) {
      pair[side].measurement = { gapClock: null };
      delete pair[side].rendererCosts;
    }
  });
  c.rendererCostQualification = null;
  assert.ok(c.fps.paired.ratio > 0.97);
  assert.equal(qualifyProductMetrics(c).status, "inconclusive");
});

for (const [name, change, expected] of [
  ["claimed pass", c => { c.fps.paired.status = "pass"; }, /summary does not match/],
  ["changed FPS", c => { c.measurements[6].candidate.fps.effectiveFps *= 1.1; }, /summary does not match/],
  ["missing pair", c => { c.measurements.pop(); }, /missing prescribed/],
  ["three-pair-only evidence", c => { c.protocol.pairs = 3; }, /seven-pair/],
  ["stale reported order", c => { c.protocol.order[3].reverse(); }, /alternating order/],
  ["changed pair role", c => { c.measurements[5].candidate.pair.position = 2; }, /metadata changed/],
  ["relaxed FPS budget", c => { c.thresholds.strictMinFpsRatio = 0.8; }, /budget changed/],
  ["missing render cost", c => { delete c.measurements[0].candidate.rendererCosts; }, /finite and positive/],
  ["invented render status", c => { c.rendererCostQualification.renderMs.ratio = 0.8; }, /summary does not match/],
  ["missing render summary", c => { c.rendererCostQualification = null; }, /summary does not match/],
]) {
  test(`product qualification rejects ${name}`, () => {
    const c = metricComparison({ fpsRatios: noisy }); change(c);
    assert.throws(() => qualifyProductMetrics(c), expected);
  });
}

test("paired intervals reject malformed data rather than manufacturing a pass", () => {
  for (const after of [[1, 1], [1, 0, 1], [1, NaN, 1], [1, Infinity, 1], [1, "1", 1]]) {
    assert.throws(() => pairedLogInterval([1, 1, 1], after));
  }
  assert.throws(() => pairedLogInterval(undefined, undefined), /arrays/);
});
