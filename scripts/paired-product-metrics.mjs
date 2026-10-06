import assert from "node:assert/strict";

const TWO_SIDED_T95 = new Map([
  [3, 4.302652729696142],
  [7, 2.446911848791681],
]);

export function pairedLogInterval(before, after) {
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
