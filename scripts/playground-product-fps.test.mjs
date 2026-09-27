import assert from "node:assert/strict";
import test from "node:test";
import { sampleRendererFps } from "./playground-product-fps.mjs";

function sample({ now, metricAt = now, frames, time, ...state }) {
  return { now, metricAt, frames, time, phase: "source", ready: true, needsPresent: false,
    bufferedDeltas: 0, runInFlight: true, playbackControls: "unavailable", ...state };
}

test("product FPS ignores a cold setup epoch reset before the full authored pass", () => {
  const samples = [
    sample({ now: 0, frames: 6, time: 0 }), sample({ now: 400, frames: 12, time: 0.4 }),
    sample({ now: 900, frames: 20, time: 0.9 }), sample({ now: 1200, frames: 6, time: 0 }),
    sample({ now: 1400, frames: 10, time: 0.2 }), sample({ now: 1700, frames: 16, time: 0.5 }),
    sample({ now: 2000, frames: 22, time: 0.8 }), sample({ now: 2300, frames: 28, time: 1.1 }),
    sample({ now: 2700, frames: 36, time: 1.5 }), sample({ now: 3000, frames: 42, time: 1.8 }),
    sample({ now: 3300, frames: 48, time: 2.1 }), sample({ now: 3700, frames: 56, time: 2.5 }),
    sample({ now: 4000, frames: 62, time: 2.8 }),
    sample({ now: 4200, frames: 66, time: 3, phase: "endpoint" }),
  ];
  const fps = sampleRendererFps(samples, 3, { minMeasurementMs: 1_000 });
  assert.deepEqual({ startFrames: fps.startFrames, endFrames: fps.endFrames, epochCount: fps.epochCount },
    { startFrames: 6, endFrames: 66, epochCount: 2 });
  assert.equal(fps.effectiveFps, 20);
});

test("product FPS rejects a cold epoch that never completes the authored pass", () => {
  const samples = Array.from({ length: 10 }, (_, index) => sample({ now: index * 100, frames: index + 1, time: index / 10 }));
  assert.throws(() => sampleRendererFps(samples, 3), /no settled renderer epoch covered/);
});

test("product FPS discards stale and unsettled metric replies", () => {
  const samples = [sample({ now: 0, frames: 1, time: 0, ready: false }),
    ...Array.from({ length: 10 }, (_, index) => sample({
      now: 100 + index * 120,
      metricAt: 10 + index,
      frames: index + 1,
      time: index / 3,
    })),
    sample({ now: 200, frames: 2, time: 1, metricAt: 10 }),
    sample({ now: 1200, frames: 22, time: 3, metricAt: 21, phase: "endpoint" })];
  const fps = sampleRendererFps(samples, 3, { minMeasurementMs: 1_000 });
  assert.equal(fps.startFrames, 1);
  assert.equal(fps.endFrames, 22);
});

test("an unsettled reset still separates renderer counters", () => {
  const samples = [
    ...Array.from({ length: 10 }, (_, index) => sample({
      now: index * 100, frames: 100 + index, time: index / 10,
    })),
    sample({ now: 1000, frames: 0, time: 0, ready: false }),
    ...Array.from({ length: 10 }, (_, index) => sample({
      now: 2000 + index * 150, frames: 200 + index, time: 0.9 + index * 2.1 / 9,
    })),
  ];
  const fps = sampleRendererFps(samples, 3);
  assert.equal(fps.epochCount, 2);
  assert.equal(fps.startFrames, 200);
  assert.equal(fps.endFrames, 209);
});

test("product FPS rejects telemetry beyond the authored endpoint", () => {
  const samples = Array.from({ length: 10 }, (_, index) => sample({
    now: index * 150, frames: index + 1, time: index * 3.5 / 9,
  }));
  assert.throws(() => sampleRendererFps(samples, 3), /no settled renderer epoch covered/);
});
