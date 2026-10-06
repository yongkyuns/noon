// Independent raster expectations, not used by either authoring frontend.
import assert from "node:assert/strict";
export function assertLiveCoordinatePixels(png, time, regionCount) {
  const green = (r,g,b) => g > r + 25 && g > b + 25;
  const orange = (r,g,b) => r > g + 20 && g > b + 20;
  const blue = (r,g,b) => b > r + 35 && g > r + 20;
  assert.ok(regionCount(png,-5,2.6,green)>5, "unrelated sentinel moved or disappeared");
  assertLiveAxisCoverage(png, time);
  if (time < 0.25) {
    assert.equal(regionCount(png,0,2,orange),0,"NumberLine appeared before construction");
    return;
  }
  assert.ok(regionCount(png,0,2,orange)>2,"late NumberLine shaft missing");
  if (time >= 0.75) {
    const alpha = Math.min(1,(time-0.75)/0.5);
    const point = a => [-3.5+8*a,-2+3*a];
    if (alpha>0) assert.ok(regionCount(png,...point(alpha/2),blue)>2,
      "new curve did not use the moved axes coordinate frame");
    if (alpha<1) assert.equal(regionCount(png,...point(alpha+(1-alpha)*0.75),blue),0,
      "new curve revealed future geometry early");
  }
}

// The fixture's white y-axis has a 0.02 screen-space stroke, viewed at
// 540 / 8 pixels per unit: a 1.35-pixel-wide strip. At an integer x its
// coverage is split between two pixels; Cairo produces 173 in each, so
// counting only channels above 180 incorrectly calls that axis absent.
// Check the independently specified strip's pixel-box overlap instead.
// This also rejects a wrong position, width, color, opacity or early axis.
export function assertLiveAxisCoverage(png, time) {
  assert.ok(Number.isFinite(time) && time >= 0, "invalid live coordinate sample time");
  assert.equal(png.width, 960);
  assert.equal(png.height, 540);
  assert.equal(png.data.length, png.width * png.height * 4, "expected complete RGBA pixels");
  const pixelsPerUnit = png.height / 8;
  const shift = 0.5 * Math.min(1, Math.max(0, (time - 0.25) / 0.5));
  const centerX = png.width / 2 + shift * pixelsPerUnit;
  const centerY = Math.round(png.height / 2 - 0.8 * pixelsPerUnit);
  const halfWidth = 0.02 * pixelsPerUnit / 2;
  for (let y = centerY - 3; y <= centerY + 3; y++) {
    for (let x = Math.floor(centerX) - 3; x <= Math.ceil(centerX) + 3; x++) {
      const coverage = time < 0.25 ? 0 : Math.max(0,
        Math.min(x + 1, centerX + halfWidth) - Math.max(x, centerX - halfWidth));
      const i = (y * png.width + x) * 4;
      for (let channel = 0; channel < 3; channel++) {
        // Two byte levels cover subpixel raster/UNORM rounding at an edge;
        // untouched black background and pre-construction pixels stay exact.
        const allowance = coverage > 0 && coverage < 1 ? 2 : 0;
        assert.ok(Math.abs(png.data[i + channel] - 255 * coverage) <= allowance,
          `live axis coverage at ${time}s, pixel (${x}, ${y}), channel ${channel}: ` +
          `${png.data[i + channel]} != ${255 * coverage}`);
      }
      assert.equal(png.data[i + 3], 255, "live axis capture must remain opaque");
    }
  }
}
