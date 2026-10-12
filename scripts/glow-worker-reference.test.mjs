import test from "node:test";
import assert from "node:assert/strict";
import { referenceGlow, maxChannelError } from "./glow-worker-reference.mjs";

function inputs() {
  const width = 5, height = 5, padding = 5, w = 15;
  const images = Array.from({ length: 6 }, () => ({ width: w, height: w, data: Buffer.alloc(w * w * 4) }));
  images[3].data[(7 * w + 7) * 4] = 255;
  return { images, sample: { sigma: 1, intensity: 1, tint: [1, 1, 1, 1], opacity: 1 },
    size: { width, height, padding } };
}

test("independent square-support Gaussian keeps its complete normalization", () => {
  const { images, sample, size } = inputs();
  const result = referenceGlow(images, sample, size);
  assert.equal(result.data[(2 * 5 + 2) * 4], 41);
  assert.equal(result.data[(2 * 5 + 3) * 4], 25);
  assert.equal(result.data[(3 * 5 + 3) * 4], 15);
  assert.equal(result.haloSignal, 41);
  assert.ok(maxChannelError(result.data, result.ordinary) > 2);
});

test("neutral intensity makes the halo exactly ordinary", () => {
  const { images, sample, size } = inputs();
  const result = referenceGlow(images, { ...sample, intensity: 0 }, size);
  assert.equal(maxChannelError(result.data, result.ordinary), 0);
  assert.equal(result.haloSignal, 0);
});

test("a foreground pixel covers the glow at the original painter position", () => {
  const { images, sample, size } = inputs();
  const center = (7 * 15 + 7) * 4;
  images[4].data[center + 1] = 255;
  images[5].data[center] = 255;
  const result = referenceGlow(images, sample, size);
  assert.deepEqual([...result.data.subarray((2 * 5 + 2) * 4, (2 * 5 + 2) * 4 + 4)], [0, 255, 0, 255]);
});

test("reference rejects missing image data and truncated capture support", () => {
  const { images, sample, size } = inputs();
  assert.throws(() => referenceGlow(images.slice(1), sample, size), /dimensions/);
  assert.throws(() => referenceGlow(images, { ...sample, sigma: 64 }, size), /padding/);
  assert.throws(() => maxChannelError(Buffer.alloc(0), Buffer.alloc(0)), /empty/);
  assert.throws(() => maxChannelError(Buffer.alloc(4), Buffer.alloc(8)), /mismatched/);
});
