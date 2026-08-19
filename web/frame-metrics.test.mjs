import assert from "node:assert/strict";
import test from "node:test";

import { FrameMetrics, summarizeSamples } from "./frame-metrics.js";

test("summarizes deterministic frame percentiles", () => {
  assert.deepEqual(summarizeSamples([4, 1, 3, 2]), {
    p50: 2,
    p95: 4,
    max: 4,
    mean: 2.5,
  });
  assert.equal(summarizeSamples([]), null);
});

test("records submission time separately from presentation cadence", () => {
  const metrics = new FrameMetrics();
  metrics.record(100, 0.5);
  metrics.record(116, 0.75);
  metrics.record(133, 1.0);

  assert.deepEqual(metrics.summary(), {
    frames: 3,
    submission: { p50: 0.75, p95: 1, max: 1, mean: 0.75 },
    interval: { p50: 16, p95: 17, max: 17, mean: 16.5 },
  });
  metrics.reset();
  assert.deepEqual(metrics.summary(), {
    frames: 0,
    submission: null,
    interval: null,
  });
});

test("rejects non-finite measurements", () => {
  const metrics = new FrameMetrics();
  assert.throws(() => metrics.record(Number.NaN, 1), /finite timestamps/);
  assert.throws(() => metrics.record(1, Number.POSITIVE_INFINITY), /finite timestamps/);
});
