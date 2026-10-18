import assert from "node:assert/strict";
import test from "node:test";
import { inspectSignalTrain } from "../scripts/signal-trains-pixel-check.mjs";

function image({ rails = true, pulses = false } = {}) {
  const data = new Uint8Array(640 * 360 * 4);
  for (let i = 3; i < data.length; i += 4) data[i] = 255;
  const result = { width: 640, height: 360, data };
  for (const [rail, y] of [135, 225].entries()) {
    if (rails) for (let x = 185; x < 455; x += 1) pixel(result, x, y, [64, 64, 64]);
    if (pulses) for (let x = 200; x < 205; x += 1) pixel(result, x, y, rail === 0 ? [0, 255, 255] : [255, 204, 0]);
  }
  return result;
}
function pixel(image, x, y, rgb) { image.data.set([...rgb, 255], (y * 640 + x) * 4); }

test("accepts intact inactive rails and an active pair", () => {
  assert.deepEqual(inspectSignalTrain(image(), 0).map((x) => x.count), [0, 0]);
  assert.deepEqual(inspectSignalTrain(image({ pulses: true }), 1).map((x) => x.count), [1, 1]);
});
test("rejects matching empty initial and terminal frames", () => {
  for (const index of [0, 6, 7]) assert.throws(() => inspectSignalTrain(image({ rails: false }), index), /missing connection/);
});
test("visible pulses alone cannot substitute for retained rails", () => {
  assert.throws(() => inspectSignalTrain(image({ rails: false, pulses: true }), 1), /missing connection/);
});
test("rejects a one-column hole in either connection", () => {
  for (const y of [135, 225]) {
    const frame = image();
    pixel(frame, 320, y, [0, 0, 0]);
    assert.throws(() => inspectSignalTrain(frame, 0), /missing connection/);
  }
});
test("rejects missing signals and a stale wrong-count frame", () => {
  assert.throws(() => inspectSignalTrain(image(), 1), /signal count/);
  assert.throws(() => inspectSignalTrain(image({ pulses: true }), 4), /signal count/);
});
test("rejects pulses on the wrong rail or beyond an endpoint", () => {
  const swapped = image({ pulses: true });
  pixel(swapped, 200, 225, [0, 255, 255]);
  assert.throws(() => inspectSignalTrain(swapped, 1), /pulse left rail/);
  const escaped = image({ pulses: true });
  pixel(escaped, 455, 135, [0, 255, 255]);
  assert.throws(() => inspectSignalTrain(escaped, 1), /pulse left connection/);
});
test("rejects transparent, truncated or wrong-sized frames and unknown samples", () => {
  const transparent = image(); transparent.data[3] = 0;
  assert.throws(() => inspectSignalTrain(transparent, 0));
  const truncated = image(); truncated.data = truncated.data.subarray(4);
  assert.throws(() => inspectSignalTrain(truncated, 0));
  assert.throws(() => inspectSignalTrain({ ...image(), width: 320 }, 0));
  assert.throws(() => inspectSignalTrain(image(), 8));
});
