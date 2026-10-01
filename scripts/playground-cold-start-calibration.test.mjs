import test from "node:test";
import assert from "node:assert/strict";

import { summarizePageTargetThrottleSamples } from "./playground-cold-start-calibration.mjs";

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
