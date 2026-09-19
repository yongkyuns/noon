// External animation fixtures/oracles, never part of scene execution.
import assert from "node:assert/strict";
import { animatedPlottingCases } from "./animated-plotting-samples.mjs";
export { animatedPlottingCases, animatedPresentedTime } from "./animated-plotting-samples.mjs";

function countColor(png, color) {
  let count = 0;
  for (let i = 0; i < png.data.length; i += 4) {
    if (color(png.data[i], png.data[i + 1], png.data[i + 2])) count++;
  }
  return count;
}
const blue = (r, g, b) => b > r + 35 && g > r + 20;
const yellow = (r, g, b) => r > b + 40 && g > b + 40;
export function assertAnimatedPlotPixels(png, time, mode, regionCount) {
  if (mode === "coordinates") {
    const bluePixels = countColor(png, blue), yellowPixels = countColor(png, yellow);
    if (time <= 1.3) assert.equal(bluePixels, 0, "curve appeared before Create");
    if (time <= 3.3) assert.equal(yellowPixels, 0, "data appeared before its Create");
    if (time >= 1.8) assert.ok(bluePixels > 100, "curve failed to reveal");
    if (time >= 3.8) assert.ok(yellowPixels > 100, "sampled data failed to reveal");
    // At the curve midpoint, its left portion exists and its far right does not.
    if (time === 2.3) {
      assert.ok(regionCount(png, -4, Math.sin(0.8) * 4 / 3, blue) > 2);
      assert.equal(regionCount(png, 4, Math.sin(7.2) * 4 / 3, blue), 0);
    }
    return;
  }
  if (time <= 1.5) assert.equal(countColor(png, yellow), 0, "marker appeared before FadeIn");
  if (time < 2) return;
  let x, y = 0;
  if (time <= 3.5) x = -4.4 + 4.4 * (time - 2) / 1.5;
  else if (time <= 4) x = 0;
  else if (time <= 5.2) x = 3.3 * (time - 4) / 1.2;
  else if (time <= 6.5) { x = 3.3; y = 0.6 * Math.max(0, (time - 5.7) / 0.8); }
  else { x = 3.3 - 5.5 * Math.min(1, (time - 6.5) / 1.5); y = 0.6; }
  assert.ok(regionCount(png, x, y, yellow) > 20,
    `marker missed NumberLine coordinate at ${time}: (${x}, ${y})`);
}

export function assertAnimatedPlotHolds(captures, mode) {
  for (const [start, end] of animatedPlottingCases[mode].waits) {
    const before = captures.find(capture => capture.time === start);
    assert.ok(before, "missing start-of-wait image");
    for (const after of captures.filter(capture => capture.time > start && capture.time <= end)) {
      assert.deepEqual(after.png.data, before.png.data, `wait changed the picture at ${after.time}`);
    }
  }
  if (mode === "coordinates") {
    const at = time => captures.find(capture => capture.time === time).png;
    assert.ok(countColor(at(2.3), blue) < countColor(at(3.3), blue) * 0.8,
      "Create must reveal the curve over time, not show the full path immediately");
  }
}
