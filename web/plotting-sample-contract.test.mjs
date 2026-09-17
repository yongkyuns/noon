import assert from "node:assert/strict";
import test from "node:test";
import {
  assertSampleReceipt, isPresentedSample, pythonPresentedTime,
} from "../scripts/plotting-sample-contract.mjs";

const presented = time => ({ ready: true, retained: true, presentedFrames: 3, time });

test("quiet waits preserve presented time but still require an exact engine acknowledgement", () => {
  for (const [requested, frame] of [[0, 0], [0.125, 0], [1.375, 1.25], [1.5, 1.25]]) {
    assertSampleReceipt({ time: requested }, requested);
    assert.equal(pythonPresentedTime(requested, true), frame);
    assert.ok(isPresentedSample(presented(frame), frame));
    if (frame !== requested) {
      assert.throws(() => assertSampleReceipt({ time: frame }, requested), /acknowledgement/);
      assert.equal(isPresentedSample(presented(frame), requested), false);
    }
  }
});

test("animated samples and wait boundaries still require their exact presented time", () => {
  for (const time of [0.25, 0.5, 0.75, 1, 1.25]) {
    assert.equal(pythonPresentedTime(time, true), time);
    assert.ok(isPresentedSample(presented(time), pythonPresentedTime(time, true)));
    assert.equal(isPresentedSample(presented(time - 0.125), pythonPresentedTime(time, true)), false);
    assert.equal(isPresentedSample(presented(time + 0.125), pythonPresentedTime(time, true)), false);
  }
});

test("quiet-wait policy cannot leak into the other plotting fixtures", () => {
  for (const time of [0.125, 1.375, 1.5, 1.8, 4.5, 6]) {
    assert.equal(pythonPresentedTime(time), time);
    assert.equal(isPresentedSample(presented(time - 0.125), pythonPresentedTime(time)), false);
  }
  assert.equal(pythonPresentedTime(1.75, true), 1.75);
});

test("acknowledgements reject absent, nonfinite, stale and future times", () => {
  for (const receipt of [null, {}, { time: NaN }, { time: Infinity }, { time: "0.5" },
    { time: 0.25 }, { time: 0.75 }]) {
    assert.throws(() => assertSampleReceipt(receipt, 0.5), /acknowledgement/);
  }
  for (const time of [-1, NaN, Infinity]) {
    assert.throws(() => assertSampleReceipt({ time }, time));
    assert.throws(() => pythonPresentedTime(time, true));
  }
});

test("no frame is accepted before presentation or without retained readiness", () => {
  for (const override of [{ ready: false }, { retained: false }, { presentedFrames: 0 },
    { presentedFrames: 0.5 }, { presentedFrames: NaN }, { time: NaN }, { time: undefined }]) {
    assert.equal(isPresentedSample({ ...presented(0), ...override }, 0), false);
  }
  assert.equal(isPresentedSample(null, 0), false);
});

test("reported numeric times are not rewritten or coerced by the checks", () => {
  const receipt = Object.freeze({ time: 0.5 - 1e-10 });
  const metrics = Object.freeze(presented(0.5 + 1e-10));
  assertSampleReceipt(receipt, 0.5);
  assert.ok(isPresentedSample(metrics, 0.5));
  assert.equal(receipt.time, 0.5 - 1e-10);
  assert.equal(metrics.time, 0.5 + 1e-10);
});
