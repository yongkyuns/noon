import assert from "node:assert/strict";
import test from "node:test";
import { evaluateBudget, validateRuntimeExpectations } from "./perf-corpus-budget.mjs";

const report = {
  cadence: { frameIntervalMs: { p95: 17, p99: 20 }, effective: { longFrameRate: 0.01 } },
  pipeline: { advanceRoundTripMs: { p95: 5 } },
};

test("unavailable CPU timing cannot silently pass an enforced budget", () => {
  const result = evaluateBudget(report, { frameIntervalP95Ms: 20, cpuFrameP95Ms: 6 });
  assert.equal(result.passed, false);
  assert.equal(result.complete, false);
  assert.equal(result.checks[1].status, "unavailable");
  assert.equal(result.checks[1].actual, null);
});

test("round-trip and cadence budgets evaluate their own measurements", () => {
  const result = evaluateBudget(report, { advanceRoundTripP95Ms: 4, frameIntervalP95Ms: 20 });
  assert.equal(result.complete, true);
  assert.equal(result.passed, false);
  assert.deepEqual(result.checks.map((check) => check.status), ["failed", "passed"]);
  assert.equal(evaluateBudget(report, { advanceRoundTripP95Ms: 6 }).passed, true);
  assert.equal(evaluateBudget(report, null).gated, false);
});

test("manifest runtime expectations reject empty scenes and lost source continuation", () => {
  const definition = { minimumObjects: 4, sourceContinuation: true };
  const report = { scene: { objects: 4 }, execution: { sourceContinuation: true } };
  assert.deepEqual(validateRuntimeExpectations(report, definition), { passed: true, failures: [] });
  assert.deepEqual(validateRuntimeExpectations({ scene: { objects: 0 }, execution: { sourceContinuation: false } }, definition), {
    passed: false,
    failures: ["objects 0 < 4", "sourceContinuation false != true"],
  });
  assert.equal(validateRuntimeExpectations({ scene: {}, execution: {} }, definition).passed, false);
  assert.equal(validateRuntimeExpectations({}, {}).passed, true);
});
