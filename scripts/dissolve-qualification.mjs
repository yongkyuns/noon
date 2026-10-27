// A high-level gesture must mask the final authored halo as well as the family.
// The independent field and source AABB below come from the authored fixture;
// no engine uniforms or mask output are used to construct expected coverage.
import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

function lattice(x, y, seed) {
  let h = Math.imul(x, 0x9e3779b9) ^ Math.imul(y, 0x85ebca6b) ^ seed;
  h = Math.imul(h ^ (h >>> 16), 0x7feb352d);
  h = Math.imul(h ^ (h >>> 15), 0x846ca68b);
  return ((h ^ (h >>> 16)) >>> 8) / 16777216;
}
const mix = (a, b, t) => a * (1 - t) + b * t;
function coverage(x, y, progress) {
  if (progress <= 0) return 1;
  if (progress >= 1) return 0;
  const px = x * 16, py = y * 16, bx = Math.floor(px), by = Math.floor(py);
  const fx = px - bx, fy = py - by;
  const wx = fx * fx * (3 - 2 * fx), wy = fy * fy * (3 - 2 * fy);
  const noise = mix(mix(lattice(bx, by, 7), lattice(bx + 1, by, 7), wx),
    mix(lattice(bx, by + 1, 7), lattice(bx + 1, by + 1, 7), wx), wy);
  const t = Math.max(0, Math.min(1, (progress - noise) / 0.2 + 0.5));
  return 1 - t * t * (3 - 2 * t);
}
function progress(time) {
  if (time < 0.5 || (time >= 3 && time < 3.5) || time >= 6) return 0;
  if (time <= 1.5) return time - 0.5;
  if (time < 2) return 1;
  if (time < 3) return 3 - time;
  if (time <= 4.5) return time - 3.5;
  if (time < 5) return 1;
  return 6 - time;
}
function reference(image, time) {
  assert.equal(image.width, 640); assert.equal(image.height, 360);
  const scale = 45, shift = 4 * scale;
  const min = [320 + 1.2 * scale, 180 - 0.4 * scale];
  const p = progress(time);
  let maximumError = 0, channels = 0, haloWitness = 0, sourceWitness = 0;
  for (let y = 140; y < 220; y++) for (let x = 344; x < 456; x++) {
    const source = image.data[(y * 640 + x - shift) * 4];
    sourceWitness = Math.max(sourceWitness, source);
    if (x < min[0] || x >= min[0] + 1.6 * scale || y < min[1] || y >= min[1] + 0.8 * scale) {
      haloWitness = Math.max(haloWitness, source);
    }
    const expected = Math.round(source * coverage((x + 0.5 - min[0]) / (1.6 * scale),
      (y + 0.5 - min[1]) / (0.8 * scale), p));
    for (let c = 0; c < 3; c++) {
      maximumError = Math.max(maximumError, Math.abs(expected - image.data[(y * 640 + x) * 4 + c]));
      channels++;
    }
  }
  assert.ok(sourceWitness > 200);
  assert.ok(haloWitness > 3, "fixture must expose the authored off-source halo");
  assert.ok(maximumError <= 2, `Dissolve at ${time}: maximum byte error ${maximumError}`);
  if (p === 0 || p === 1) assert.equal(maximumError, 0, "visibility endpoints include the entire authored halo");
  return { channels, maximumError, haloWitness, progress: p };
}

await qualifyPairedAuthoring({
  cases: [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2, 2.25, 2.5, 3, 3.25,
    3.5, 4, 4.5, 4.75, 5, 5.5, 6, 6.25, 6.5].map(sampleTime => ({
    id: `dissolve-${sampleTime}`, file: "dissolve.py", factory: "createDirectDissolveSmokeRenderer",
    duration: 6.5, sampleTime, playback: "live", canvasSize: [640, 360],
    directHeldSampleTime: ({ 0.25: 0, 1.75: 1.5, 3.25: 3, 4.75: 4.5, 6.25: 6 })[sampleTime],
    boundaries: [0.5, 1.5, 2, 3, 3.5, 4.5, 5, 6], stopAtSourceCompletion: sampleTime === 6.5,
  })),
  artifactDirectory: "browser-smoke-artifacts/dissolve",
  prepareContext: disableAuthoringJspi,
  qualifyPixels({ fixture, rust, python }) {
    return { rust: reference(rust, fixture.sampleTime), python: reference(python, fixture.sampleTime) };
  },
});
