import assert from "node:assert/strict";

// Fixed adjacent-base and cumulative-anchor cohorts share the same strict estimator.
export const PRODUCT_PAIR_COUNT = 7;
export const CUMULATIVE_PRODUCT_PAIR_COUNT = 3;

export function productPairOrder(index) {
  assert.ok(Number.isSafeInteger(index) && index >= 1 && index <= PRODUCT_PAIR_COUNT,
    "product pair index must be between one and seven");
  return index % 2 === 0 ? ["candidate", "baseline"] : ["baseline", "candidate"];
}

export function readProductPair(env) {
  const index = env.NOON_PRODUCT_PAIR_INDEX;
  const position = env.NOON_PRODUCT_PAIR_POSITION;
  if (index === undefined && position === undefined) return null;
  assert.match(index ?? "", /^[1-7]$/, "product pair index must be between one and seven");
  assert.match(position ?? "", /^[12]$/, "product pair position must be one or two");
  const pair = { index: Number(index), position: Number(position) };
  assert.equal(productPairOrder(pair.index)[pair.position - 1], env.NOON_PRODUCT_LABEL ?? "candidate",
    "product pair role must follow the declared alternating order");
  return pair;
}

const TWO_SIDED_T95 = new Map([
  [3, 4.302652729696142],
  [7, 2.446911848791681],
]);

export function pairedLogInterval(before, after) {
  assert.ok(Array.isArray(before) && Array.isArray(after), "paired measurements must be arrays");
  assert.equal(after.length, before.length, "paired measurements changed length");
  assert.ok(TWO_SIDED_T95.has(before.length), "unsupported paired sample count");
  assert.ok([...before, ...after].every(value => Number.isFinite(value) && value > 0),
    "paired measurements must be finite and positive");
  const logs = before.map((value, index) => Math.log(after[index] / value));
  const mean = logs.reduce((sum, value) => sum + value, 0) / logs.length;
  const variance = logs.reduce((sum, value) => sum + (value - mean) ** 2, 0) / (logs.length - 1);
  const half = TWO_SIDED_T95.get(logs.length) * Math.sqrt(variance / logs.length);
  return {
    ratio: Math.exp(mean),
    lower: Math.exp(mean - half),
    upper: Math.exp(mean + half),
    before,
    after,
  };
}

export function pairedThroughput(before, after, minRatio = 0.97) {
  assert.ok(Number.isFinite(minRatio) && minRatio > 0 && minRatio <= 1,
    "minimum throughput ratio must be in (0, 1]");
  const interval = pairedLogInterval(before, after);
  return {
    ...interval,
    status: interval.upper < minRatio ? "regression"
      : interval.lower >= minRatio ? "pass" : "inconclusive",
  };
}

export function pairedCostConfidence(before, after, maxPointRatio = 1.03, maxUpperRatio = 1.05) {
  assert.ok(Number.isFinite(maxPointRatio) && maxPointRatio >= 1,
    "maximum point cost ratio must be at least one");
  assert.ok(Number.isFinite(maxUpperRatio) && maxUpperRatio >= maxPointRatio,
    "maximum upper cost ratio must not be tighter than the point ratio");
  const interval = pairedLogInterval(before, after);
  return {
    ...interval,
    status: interval.lower > maxPointRatio ? "regression"
      : interval.ratio <= maxPointRatio && interval.upper <= maxUpperRatio ? "pass" : "inconclusive",
  };
}

// Recompute qualifications from the retained runs, rather than trusting a status
// label in comparison.json. Renderer cost and delivered FPS are independent
// requirements: neither a pass nor a speedup in one clears uncertainty in the other.
export function qualifyProductMetrics(comparison, expectedPairs = PRODUCT_PAIR_COUNT) {
  // The caller pins the scope; an artifact must not choose its own sample count.
  assert.ok([PRODUCT_PAIR_COUNT, CUMULATIVE_PRODUCT_PAIR_COUNT].includes(expectedPairs),
    "unsupported product qualification pair count");
  const protocolName = expectedPairs === PRODUCT_PAIR_COUNT ? "seven-pair" : "three-pair";
  assert.equal(comparison.protocol?.pairs, expectedPairs, `missing ${protocolName} product protocol`);
  assert.equal(comparison.measurements?.length, expectedPairs, "missing prescribed product pair");
  assert.deepEqual(comparison.protocol.order,
    Array.from({ length: expectedPairs }, (_, i) => productPairOrder(i + 1)),
    "product protocol must retain the actual alternating order");
  assert.equal(comparison.thresholds?.strictMinFpsRatio, 0.97, "strict FPS budget changed");
  const pairs = comparison.measurements;
  const measurement = pairs[0].baseline.measurement;
  assert.ok(measurement && (measurement.gapClock === null || typeof measurement.gapClock === "string"),
    "missing product measurement protocol");
  for (const [i, pair] of pairs.entries()) {
    for (const side of ["baseline", "candidate"]) {
      assert.equal(pair[side]?.label, side, "product pair role changed");
      assert.deepEqual(pair[side]?.pair,
        { index: i + 1, position: productPairOrder(i + 1).indexOf(side) + 1 },
        "product pair metadata changed");
      assert.deepEqual(pair[side]?.measurement, measurement, "product measurement protocol changed");
    }
  }
  const fps = pairedThroughput(pairs.map(p => p.baseline.fps?.effectiveFps),
    pairs.map(p => p.candidate.fps?.effectiveFps));
  assert.deepEqual(comparison.fps?.paired, fps, "paired FPS summary does not match retained runs");
  const render = measurement.gapClock === null ? null : pairedCostConfidence(
    pairs.map(p => p.baseline.rendererCosts?.cpuWallMs?.renderMs?.mean),
    pairs.map(p => p.candidate.rendererCosts?.cpuWallMs?.renderMs?.mean));
  assert.deepEqual(comparison.rendererCostQualification, render === null ? null : { renderMs: render },
    "paired renderer cost summary does not match retained runs");
  const statuses = [fps.status, ...(render === null ? [] : [render.status])];
  return { fps, render, status: statuses.includes("regression") ? "regression"
    : statuses.includes("inconclusive") ? "inconclusive" : "pass" };
}
