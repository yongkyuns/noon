import assert from "node:assert/strict";
import test from "node:test";
import { compareTipRoi, enforceTipMetrics } from "./arrow-tip-raster-metrics.mjs";

function triangle(width = 64, height = 64) {
  const data = new Uint8Array(width * height * 4);
  for (let pixel = 0; pixel < width * height; pixel += 1) data[pixel * 4 + 3] = 255;
  for (let y = 20; y < 40; y += 1) {
    for (let x = 20; x < 40; x += 1) {
      let samples = 0;
      for (let sy = 0; sy < 8; sy += 1) {
        for (let sx = 0; sx < 8; sx += 1) {
          const px = x + (sx + 0.5) / 8;
          const py = y + (sy + 0.5) / 8;
          if (px >= 21.25 && px <= 38.75 && Math.abs(py - 30.125) <= (38.75 - px) * 0.47) samples += 1;
        }
      }
      const value = Math.round(255 * samples / 64);
      data.fill(value, (y * width + x) * 4, (y * width + x) * 4 + 3);
    }
  }
  return { width, height, data };
}
const roi = { x: 18, y: 18, width: 24, height: 24 };
const clone = (image) => ({ ...image, data: image.data.slice() });
const check = (a, b) => enforceTipMetrics(compareTipRoi(a, b, roi), "synthetic-tip");

test("identical antialiased tip passes", () => {
  const image = triangle();
  check(image, clone(image));
});
test("four-sample coverage is allowed; bitwise Cairo equality is not required", () => {
  const expected = triangle();
  const actual = clone(expected);
  for (let i = 0; i < actual.data.length; i += 4) {
    const value = Math.round(Math.round(actual.data[i] / 255 * 4) / 4 * 255);
    actual.data.fill(value, i, i + 3);
  }
  check(expected, actual);
});
test("hard-thresholded jagged edges fail even when tip area is similar", () => {
  const expected = triangle();
  const actual = clone(expected);
  for (let i = 0; i < actual.data.length; i += 4) actual.data.fill(actual.data[i] >= 128 ? 255 : 0, i, i + 3);
  assert.throws(() => check(expected, actual), /antialiased edge coverage lost/);
});
test("missing head fails rather than being hidden by the frame background", () => {
  const expected = triangle(1024, 1024);
  const actual = clone(expected);
  for (let i = 0; i < actual.data.length; i += 4) actual.data.fill(0, i, i + 3);
  assert.throws(() => check(expected, actual), /ink\/area/);
});
test("a narrower or clipped head fails", () => {
  const expected = triangle();
  const actual = clone(expected);
  for (let y = 0; y < actual.height; y += 1) {
    if (y >= 27 && y <= 33) continue;
    for (let x = 0; x < actual.width; x += 1) actual.data.fill(0, (y * actual.width + x) * 4, (y * actual.width + x) * 4 + 3);
  }
  assert.throws(() => check(expected, actual));
});
test("a displaced head with unchanged area fails", () => {
  const expected = triangle();
  const actual = clone(expected);
  for (let y = 0; y < actual.height; y += 1) {
    for (let x = 0; x < actual.width; x += 1) {
      const value = x >= 4 ? expected.data[(y * actual.width + x - 4) * 4] : 0;
      actual.data.fill(value, (y * actual.width + x) * 4, (y * actual.width + x) * 4 + 3);
    }
  }
  assert.throws(() => check(expected, actual));
});
test("invalid dimensions and out-of-frame crops are rejected", () => {
  const image = triangle();
  assert.throws(() => compareTipRoi(image, triangle(65, 64), roi), /width/);
  assert.throws(() => compareTipRoi(image, image, { ...roi, x: 63 }), /bounded ROI/);
});
test("empty reference cannot pass as blank equals blank", () => {
  const image = triangle();
  assert.throws(() => compareTipRoi(image, image, { x: 0, y: 0, width: 8, height: 8 }), /visible ink/);
});
