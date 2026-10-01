import test from "node:test";
import assert from "node:assert/strict";

import {
  resolvePageTargetCpuThrottleRate,
  summarizePageTargetThrottleSamples,
} from "./playground-cold-start-calibration.mjs";

test("page CPU throttle override preserves profile defaults and accepts controlled rates", () => {
  assert.equal(resolvePageTargetCpuThrottleRate("desktop", undefined), 1);
  assert.equal(resolvePageTargetCpuThrottleRate("mobile-class", undefined), 4);
  assert.equal(resolvePageTargetCpuThrottleRate("desktop", "4"), 4);
  assert.equal(resolvePageTargetCpuThrottleRate("mobile-class", "1"), 1);
  assert.throws(() => resolvePageTargetCpuThrottleRate("desktop", "2"), /must be 1 or 4/);
  assert.throws(() => resolvePageTargetCpuThrottleRate("unknown", undefined), /unknown profile/);
});

test("page-target throttle summary averages the two middle samples", () => {
  const result = summarizePageTargetThrottleSamples([
    { requestedPageTargetRate: 1, elapsedMs: 10 },
    { requestedPageTargetRate: 4, elapsedMs: 39 },
    { requestedPageTargetRate: 4, elapsedMs: 41 },
    { requestedPageTargetRate: 1, elapsedMs: 14 },
  ]);

  assert.deepEqual(result, {
    medianElapsedMsByRequestedPageTargetRate: { "1": 12, "4": 40 },
    observedFourXToOneXRatio: 40 / 12,
  });
  assert.ok(Number.isFinite(result.observedFourXToOneXRatio));
});

test("page-target throttle summary rejects incomplete or invalid samples", () => {
  assert.throws(() => summarizePageTargetThrottleSamples([
    { requestedPageTargetRate: 1, elapsedMs: 10 },
    { requestedPageTargetRate: 4, elapsedMs: 40 },
  ]), /at least two samples at 1x/);

  assert.throws(() => summarizePageTargetThrottleSamples([
    { requestedPageTargetRate: 1, elapsedMs: 10 },
    { requestedPageTargetRate: 1, elapsedMs: 12 },
    { requestedPageTargetRate: 4, elapsedMs: 0 },
    { requestedPageTargetRate: 4, elapsedMs: 40 },
  ]), /finite and positive/);
});
