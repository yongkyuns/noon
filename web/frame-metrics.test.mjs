import assert from "node:assert/strict";
import test from "node:test";

import {
  FrameMetrics,
  SampleWindow,
  summarizeCadence,
  summarizeSamples,
} from "./frame-metrics.js";

test("summarizes deterministic frame percentiles", () => {
  assert.deepEqual(summarizeSamples([4, 1, 3, 2]), {
    min: 1,
    p50: 2,
    p95: 4,
    p99: 4,
    max: 4,
    mean: 2.5,
  });
  assert.equal(summarizeSamples([]), null);
});

test("summarizes effective FPS, long frames, and missed vsyncs", () => {
  const summary = summarizeCadence([16, 17, 33, 50], 60);
  assert.equal(summary.targetHz, 60);
  assert.ok(Math.abs(summary.targetFrameMs - 1000 / 60) < 1e-9);
  assert.ok(Math.abs(summary.effectiveFps - 1000 / 29) < 1e-9);
  assert.equal(summary.longFrames, 2);
  assert.equal(summary.veryLongFrames, 1);
  assert.equal(summary.missedVsyncs, 3);
  assert.equal(summary.longFrameRate, 0.5);
  assert.equal(summarizeCadence([], 60), null);
  assert.throws(() => summarizeCadence([16], 0), /positive finite/);
});

test("records submission time separately from presentation cadence", () => {
  const metrics = new FrameMetrics({ targetHz: 60 });
  metrics.record(100, 0.5);
  metrics.record(116, 0.75);
  metrics.record(133, 1.0);

  const summary = metrics.summary();
  assert.deepEqual(summary.submission, {
    min: 0.5,
    p50: 0.75,
    p95: 1,
    p99: 1,
    max: 1,
    mean: 0.75,
  });
  assert.deepEqual(summary.interval, {
    min: 16,
    p50: 16,
    p95: 17,
    p99: 17,
    max: 17,
    mean: 16.5,
  });
  assert.equal(summary.frames, 3);
  assert.ok(Math.abs(summary.cadence.effectiveFps - 1000 / 16.5) < 1e-9);
  assert.equal(summary.cadence.longFrames, 0);
  assert.equal(summary.cadence.missedVsyncs, 0);

  metrics.reset();
  assert.deepEqual(metrics.summary(), {
    frames: 0,
    submission: null,
    interval: null,
    cadence: null,
  });
});

test("bounded sample windows retain recent measurements", () => {
  const samples = new SampleWindow(3);
  samples.record(1);
  samples.record(2);
  samples.record(3);
  samples.record(10);

  assert.equal(samples.size, 3);
  assert.deepEqual(samples.summary(), {
    min: 2,
    p50: 3,
    p95: 10,
    p99: 10,
    max: 10,
    mean: 5,
  });
  samples.reset();
  assert.equal(samples.size, 0);
  assert.equal(samples.summary(), null);
  assert.throws(() => new SampleWindow(0), /positive integer/);
  assert.throws(() => samples.record(Number.NaN), /finite values/);
});

test("rejects invalid frame-metric inputs", () => {
  const metrics = new FrameMetrics();
  assert.throws(() => metrics.record(Number.NaN, 1), /finite timestamps/);
  assert.throws(() => metrics.record(1, Number.POSITIVE_INFINITY), /finite timestamps/);
  assert.throws(() => new FrameMetrics({ targetHz: Number.NaN }), /positive finite/);
});
