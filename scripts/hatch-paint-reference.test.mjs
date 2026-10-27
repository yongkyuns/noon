import assert from "node:assert/strict";
import test from "node:test";
import { bandAverage, filteredHatch, hatchFootprint, localFromPixel } from "./hatch-paint-reference.mjs";

const near = (actual, expected) => assert.ok(Math.abs(actual - expected) < 1e-11, `${actual} != ${expected}`);

test("interval reference has literal point, edge, period and duty witnesses", () => {
  near(bandAverage(0, 0, 0.5), 1);
  near(bandAverage(0.25, 0, 0.5), 0);
  near(bandAverage(-0.25, 0, 0.5), 1);
  near(bandAverage(0.25, 0.2, 0.5), 0.5);
  near(bandAverage(0.5, 0.75, 0.5), 1 / 3);
  for (const center of [-7.13, 0, 0.31, 4.72]) {
    for (const footprint of [1, 4, 255, 256, 1024]) near(bandAverage(center, footprint, 0.25), 0.25);
    near(bandAverage(center, 0.03, 0), 0);
    near(bandAverage(center, 0.03, 1), 1);
  }
});

test("crosshatching is the union, and phase moves local scene-unit lines", () => {
  const options = { spacing: 0.5, width: 0.25, angle: 0, phase: [0, 0], cross: true };
  near(filteredHatch([0.25, 0.25], options, [1, 1]), 0.75);
  near(filteredHatch([0.25, 0.25], { ...options, cross: false }, [1, 1]), 0.5);
  near(filteredHatch([0.25, 0.25], options, [0, 0]), 0);
  near(filteredHatch([0.25, 0.25], { ...options, phase: [0.5, 0] }, [0, 0]), 1);
  near(filteredHatch([0.25, 0.25], { ...options, phase: [4294967296.5, 0] }, [0, 0]), 1);
});

test("pixel footprint carries resolution, rotation, reflection and nonuniform pose", () => {
  const identity = { scale: [1, 1], angle: 0, translation: [0, 0] };
  const options = { spacing: 0.5, angle: 0 };
  const initial = hatchFootprint(options, identity, 32);
  initial.forEach(v => near(v, 1 / 16));
  hatchFootprint(options, { ...identity, scale: [-2, 0.5] }, 32)
    .forEach((v, i) => near(v, [1 / 8, 1 / 32][i]));
  hatchFootprint(options, identity, 64).forEach((v, i) => near(v, initial[i] / 2));
  assert.deepEqual(localFromPixel(127, 127, 256, 256, identity), [-1 / 64, 1 / 64]);
});
