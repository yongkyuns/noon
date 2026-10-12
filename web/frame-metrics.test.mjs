import assert from "node:assert/strict";
import test from "node:test";

import {
  FrameMetrics,
  PresentationRate,
  SampleWindow,
  summarizeCadence,
  summarizeSamples,
} from "./frame-metrics.js";

test("live FPS counts renderer presentations over real time, including stalls", () => {
  const rate = new PresentationRate();
  const observe = (presentedFrames, sampledAtMs) => rate.observe({ presentedFrames, sampledAtMs }, "run");
  assert.equal(observe(500, 2000), null);
  assert.equal(observe(530, 2500), null);
  assert.equal(observe(560, 3000), 60);
  assert.equal(observe(580, 3500), 60);
  assert.equal(observe(605, 4500), 30);
  assert.equal(observe(605, 5500), 0, "no new frames must not retain the previous rate");
  assert.equal(observe(620, 10500), 3, "a long stall remains in the denominator");
  assert.equal(observe(740, 11500), 120, "the target must not cap the measured rate");
});

test("live FPS discards pause, hidden-tab, run and renderer transitions", () => {
  const rate = new PresentationRate();
  const sample = (count, time, session = "run") => rate.observe({ presentedFrames: count, sampledAtMs: time }, session);
  sample(0, 0);
  assert.equal(sample(60, 1000), 60);
  rate.reset();
  assert.equal(sample(70, 10000), null);
  assert.equal(sample(130, 11000), 60);
  assert.equal(sample(200, 12000, "replacement"), null, "a replacement may have a larger counter");
  assert.equal(sample(1, 12500, "replacement"), null, "counter reset starts a new interval");
  assert.equal(sample(61, 13500, "replacement"), 60);
  assert.equal(sample(62, 10, "replacement"), null, "worker clocks may restart");
  assert.equal(sample(NaN, 1010, "replacement"), null);
  assert.equal(sample(122, 1010, "replacement"), null);
});

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

test("sample windows forget old spikes across repeated wrap and reset", () => {
  const samples = new SampleWindow(3);
  for (const value of [100, 1, 2, 3, 4, 5, 6, 7]) samples.record(value);
  assert.equal(samples.size, 3);
  assert.equal(samples.summary().max, 7);
  assert.equal(samples.summary().mean, 6);
  samples.reset();
  for (const value of [20, 21, 22, 23]) samples.record(value);
  assert.equal(samples.summary().min, 21);
  assert.equal(samples.summary().max, 23);
});

test("rejects invalid frame-metric inputs", () => {
  const metrics = new FrameMetrics();
  assert.throws(() => metrics.record(Number.NaN, 1), /finite timestamps/);
  assert.throws(() => metrics.record(1, Number.POSITIVE_INFINITY), /finite timestamps/);
  assert.throws(() => new FrameMetrics({ targetHz: Number.NaN }), /positive finite/);
});
