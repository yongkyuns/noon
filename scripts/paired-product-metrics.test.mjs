import assert from "node:assert/strict";
import test from "node:test";
import { pairedCostConfidence, pairedThroughput } from "./paired-product-metrics.mjs";

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
