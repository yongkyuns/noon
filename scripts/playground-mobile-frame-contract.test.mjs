import assert from "node:assert/strict";
import test from "node:test";
import { isSquareToCircleTransformFrame,
  isSquareToCircleNaturalWrap, SQUARE_TO_CIRCLE_TRANSFORM_START, SQUARE_TO_CIRCLE_TRANSFORM_END,
} from "./playground-mobile-frame-contract.mjs";

test("original 3-second SquareToCircle has an authored Transform interval strictly between 1 and 2", () => {
  assert.equal(SQUARE_TO_CIRCLE_TRANSFORM_START, 1);
  assert.equal(SQUARE_TO_CIRCLE_TRANSFORM_END, 2);
  // The recorded WebKit case on integrated head 00bb0c89 jumped from
  // t=1.000 to t=1.947 under realtime catch-up. It still is a valid
  // intermediate frame and should be tested for real pixels.
  const recordedFrame = { time: 1.947, backend: "WebGPU", objectCount: 1 };
  assert.equal(isSquareToCircleTransformFrame(recordedFrame), true);
  assert.equal(isSquareToCircleTransformFrame({ time: 1.15, objectCount: 1 }), true);
  assert.equal(isSquareToCircleTransformFrame({ time: 1.98, objectCount: 1 }), true);
});

test("the first Rust time decrease must itself be the authored loop wrap", () => {
  const samples = [0, 1.1, 2.4, 0.1, 2.9, 0.02];
  let previous = null, madeForwardProgress = false, accepted = null;
  for (const time of samples) {
    if (previous !== null && time < previous) {
      accepted = isSquareToCircleNaturalWrap(previous, time, madeForwardProgress);
      break;
    }
    if (previous !== null && time > previous) madeForwardProgress = true;
    previous = time;
  }
  assert.equal(accepted, false, "an early missed wrap cannot be replaced by a later cycle");
  assert.equal(isSquareToCircleNaturalWrap(2.971, 0.005, true), true);
  assert.equal(isSquareToCircleNaturalWrap(2.4, 0.1, true), false);
  assert.equal(isSquareToCircleNaturalWrap(2.971, 0.005, false), false);
});

test("rejects creation/transform endpoints, fade-out, invalid times or missing geometry", () => {
  for (const sample of [undefined, null, {}, { time: 0, objectCount: 1 },
    { time: 1, objectCount: 1 }, { time: 2, objectCount: 1 },
    { time: 2.5, objectCount: 1 }, { time: 3, objectCount: 0 },
    { time: 1.5, objectCount: 0 }, { time: 1.5, objectCount: 2 },
    { time: 1.5, objectCount: null }, { time: NaN, objectCount: 1 },
    { time: Infinity, objectCount: 1 }, { time: "1.5", objectCount: 1 },
  ]) {
    assert.equal(isSquareToCircleTransformFrame(sample), false,
      "should reject non-intermediate frame: " + JSON.stringify(sample));
  }
});
