import assert from "node:assert/strict";
import test from "node:test";
import { assertLiveAxisCoverage } from "./live-coordinate-checks.mjs";

// Independent Cairo 1.18.4 cross-sections: opaque white stroke width 1.35
// on black at x=480, 496.875 and 513.75. The fixture's 0.02-unit axis
// and 540/8 projection give these positions at the specified times.
const cairoRows = [
  { time: 0.25, pixels: [[479, 173], [480, 173]] },
  { time: 0.5, pixels: [[496, 205], [497, 141]] },
  { time: 0.75, pixels: [[513, 237], [514, 109]] },
];
function image(pixels = []) {
  const png = { width: 960, height: 540, data: new Uint8Array(960 * 540 * 4) };
  for (let i = 3; i < png.data.length; i += 4) png.data[i] = 255;
  for (let y = 213; y <= 219; y++) {
    for (const [x, value] of pixels) {
      const i = (y * png.width + x) * 4;
      png.data.set([value, value, value, 255], i);
    }
  }
  return png;
}
for (const { time, pixels } of cairoRows) {
  test(`live axis accepts independent Cairo coverage at ${time}s`, () => {
    assertLiveAxisCoverage(image(pixels), time);
  });
}
test("quiet wait requires absence and completed translation holds its endpoint", () => {
  for (const time of [0, 0.125, 0.249]) assertLiveAxisCoverage(image(), time);
  for (const time of [1, 1.25, 1.375, 1.5]) {
    assertLiveAxisCoverage(image(cairoRows[2].pixels), time);
  }
});
test("edge quantization is bounded, not an arbitrary brightness threshold", () => {
  const roundNearest = image([[479, 172], [480, 172]]);
  assertLiveAxisCoverage(roundNearest, 0.25);
  assert.throws(() => assertLiveAxisCoverage(image([[479, 175], [480, 175]]), 0.25),
    /live axis coverage/);
});
for (const [name, time, pixels] of [
  ["missing axis", 0.25, []],
  ["one-pixel displacement", 0.25, [[480, 173], [481, 173]]],
  ["stale unshifted axis", 0.5, cairoRows[0].pixels],
  ["dimmed axis", 0.25, [[479, 100], [480, 100]]],
  ["too-thick axis", 0.25, [[479, 255], [480, 255]]],
  ["wrong subpixel position", 0.25, [[479, 141], [480, 205]]],
  ["extra coverage outside the shaft", 0.25, [[478, 1], ...cairoRows[0].pixels]],
  ["early half-covered axis", 0.125, cairoRows[0].pixels],
]) {
  test(`live axis rejects ${name}`, () => {
    assert.throws(() => assertLiveAxisCoverage(image(pixels), time), /live axis coverage/);
  });
}
for (const [name, change] of [
  ["colored shaft", png => { png.data[(216 * 960 + 479) * 4 + 1] = 0; }],
  ["missing shaft row", png => { png.data.fill(0, (216 * 960 + 479) * 4, (216 * 960 + 481) * 4); }],
  ["transparent capture", png => { png.data[(216 * 960 + 479) * 4 + 3] = 0; }],
  ["wrong viewport", png => { png.width = 961; }],
  ["truncated capture", png => { png.data = png.data.subarray(0, 200); }],
]) {
  test(`live axis rejects ${name}`, () => {
    const png = image(cairoRows[0].pixels);
    change(png);
    assert.throws(() => assertLiveAxisCoverage(png, 0.25));
  });
}
for (const time of [NaN, Infinity, -1, "0.25"]) {
  test(`live axis rejects invalid sample time ${time}`, () => {
    assert.throws(() => assertLiveAxisCoverage(image(), time), /invalid live coordinate sample time/);
  });
}
