import assert from "node:assert/strict";
import test from "node:test";

import {
  stableCameraSweepTargets,
  summarizeStableCameraProfile,
} from "./authoring-perf-scenarios.js";

test("stable camera sweep is deterministic, bounded, and closes one orbit", () => {
  const targets = stableCameraSweepTargets(4, 10);
  assert.equal(targets.length, 4);
  assert.deepEqual(targets[0], { x: 0.1, y: 0 });
  assert.ok(Math.abs(targets[1].x) < 1e-12);
  assert.equal(targets[1].y, 0.06);
  assert.equal(targets[2].x, -0.1);
  assert.ok(Math.abs(targets[2].y) < 1e-12);
  assert.ok(Math.abs(targets[3].x) < 1e-12);
  assert.equal(targets[3].y, -0.06);
});

test("stable camera profile summarizes command work without hiding topology changes", () => {
  const sample = (encodeSubmitMs, uploadBytes) => ({
    timeToVisibleMs: encodeSubmitMs + 1,
    frame: {
      runtimeMs: 0.1,
      prepareMs: 0.2,
      uploadMs: 0.3,
      encodeSubmitMs,
      uploadBytes,
      drawCalls: 2,
      instances: 1000,
    },
  });

  const summary = summarizeStableCameraProfile([sample(0.4, 64), sample(0.6, 128)]);
  assert.equal(summary.samples, 2);
  assert.equal(summary.stableDrawCalls, 2);
  assert.equal(summary.stableInstances, 1000);
  assert.equal(summary.encodeSubmitMs.min, 0.4);
  assert.equal(summary.encodeSubmitMs.max, 0.6);
  assert.equal(summary.uploadBytes.min, 64);
  assert.equal(summary.uploadBytes.max, 128);

  assert.throws(
    () =>
      summarizeStableCameraProfile([
        sample(0.4, 64),
        {
          ...sample(0.5, 64),
          frame: { ...sample(0.5, 64).frame, drawCalls: 3 },
        },
      ]),
    /changed visible draw topology/,
  );
});

test("stable camera helpers reject invalid benchmark inputs", () => {
  assert.throws(() => stableCameraSweepTargets(0, 10), /positive integer/);
  assert.throws(() => stableCameraSweepTargets(4, Number.NaN), /positive and finite/);
  assert.throws(() => summarizeStableCameraProfile([]), /at least one sample/);
  assert.throws(
    () =>
      summarizeStableCameraProfile([
        {
          timeToVisibleMs: 1,
          frame: {
            runtimeMs: 0,
            prepareMs: 0,
            uploadMs: 0,
            encodeSubmitMs: 0,
            uploadBytes: 0,
            drawCalls: Number.NaN,
            instances: 1,
          },
        },
      ]),
    /invalid drawCalls/,
  );
});
