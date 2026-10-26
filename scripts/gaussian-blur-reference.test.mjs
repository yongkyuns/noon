import assert from "node:assert/strict";
import test from "node:test";
import { referenceRgbaGaussian, referenceRgbaGaussianAt } from "./gaussian-blur-reference.mjs";

test("RGBA oracle matches the literal rational 7x7 impulse field", () => {
  const bytes = new Uint8Array(7 * 7 * 4);
  bytes.set([128, 64, 32, 255], (3 * 7 + 3) * 4);
  const actual = referenceRgbaGaussian(bytes, 7, 7, 1 / Math.sqrt(2 * Math.log(2)));
  const taps = [1, 32, 256, 512, 256, 32, 1];
  for (let y = 0; y < 7; y++) for (let x = 0; x < 7; x++) {
    const point = referenceRgbaGaussianAt(bytes, 7, 7, 1 / Math.sqrt(2 * Math.log(2)), x, y);
    for (const [channel, byte] of [128, 64, 32, 255].entries()) {
      const expected = byte / 255 * taps[x] * taps[y] / 1188100;
      assert.ok(Math.abs(actual[(y * 7 + x) * 4 + channel] - expected) < 1e-14);
      assert.ok(Math.abs(point[channel] - expected) < 1e-14);
    }
  }
});

test("zero extension and final opacity do not invent opaque or grayscale color", () => {
  const bytes = new Uint8Array(5 * 5 * 4);
  for (let at = 0; at < bytes.length; at += 4) bytes.set([40, 20, 10, 80], at);
  const full = referenceRgbaGaussian(bytes, 5, 5, 1);
  const faded = referenceRgbaGaussian(bytes, 5, 5, 1, 0.37);
  assert.ok(full[3] < 80 / 255 - 0.05, "clamped/edge-renormalized negative control");
  assert.ok(Math.abs(full[0] * 2 - full[3]) < 1e-14);
  assert.ok(Math.abs(full[1] * 4 - full[3]) < 1e-14);
  assert.ok(Math.abs(full[2] * 8 - full[3]) < 1e-14);
  for (let at = 0; at < full.length; at++) assert.ok(Math.abs(faded[at] - 0.37 * full[at]) < 1e-14);
  assert.ok(full[0] < full[3] - 0.05, "alpha-only/white-mask negative control");
  assert.ok(full[3] < 0.5, "opaque-alpha negative control");
});

test("neutral output is exact normalized source and invalid inputs fail", () => {
  assert.deepEqual(referenceRgbaGaussian([12, 3, 7, 18], 1, 1, 0), [12, 3, 7, 18].map(v => v / 255));
  for (const sigma of [NaN, Infinity, -1, 64.1]) assert.throws(() => referenceRgbaGaussian([0, 0, 0, 0], 1, 1, sigma));
});
