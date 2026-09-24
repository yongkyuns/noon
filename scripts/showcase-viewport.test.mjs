import assert from "node:assert/strict";
import test from "node:test";
import { assertReplayViewport, layoutReplayViewport } from "./showcase-viewport.mjs";

const size = { width: 960, height: 540 };
const observation = () => ({ bounds: { x: 0, y: 0, ...size }, bitmap: { ...size }, deviceScaleFactor: 1 });

test("exact integer-origin CSS bounds and actual backing dimensions are required", () => {
  assert.doesNotThrow(() => assertReplayViewport(observation(), size));
  for (const field of ["x", "y", "width", "height"]) {
    const changed = observation(); changed.bounds[field] += 0.25;
    assert.throws(() => assertReplayViewport(changed, size), /integer-aligned/);
  }
  for (const field of ["width", "height"]) {
    const changed = observation(); changed.bitmap[field] += 1;
    assert.throws(() => assertReplayViewport(changed, size), /backing bitmap/);
  }
});

test("scaled output is rejected instead of being resized into a passing image", () => {
  const changed = observation(); changed.deviceScaleFactor = 2;
  assert.throws(() => assertReplayViewport(changed, size), /one device pixel/);
});

test("invalid viewport requests cannot modify the page", async () => {
  for (const invalid of [null, {}, { width: 0, height: 540 }, { width: 960.5, height: 540 },
    { width: 960, height: Infinity }, { width: -960, height: 540 }]) {
    let calls = 0;
    await assert.rejects(layoutReplayViewport({ evaluate: async () => calls++ }, invalid), /invalid replay viewport/);
    assert.equal(calls, 0);
  }
});
