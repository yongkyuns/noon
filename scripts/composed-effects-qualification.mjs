// Full shared Rust/WASM and real Python-worker paths. The ordinary white left
// composition supplies alpha; the outer Glow reference uses independent 2D math.
// Geometry/image rasterization and inner Blur coverage are shared, not independent.
import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";
import { referenceRgbaGaussian } from "./gaussian-blur-reference.mjs";

function reference(image, time, animated) {
  // Green is unchanged by the pure-red outer halo. Match the two ordinary
  // coverage intervals to obtain the actual camera's integer translation.
  let leftMin = image.width, leftMax = -1, rightMin = image.width, rightMax = -1;
  for (let y = 0; y < image.height; y++) for (let x = 0; x < image.width; x++) {
    if (image.data[(y * image.width + x) * 4 + 1] === 0) continue;
    if (x < image.width / 2) { leftMin = Math.min(leftMin, x); leftMax = Math.max(leftMax, x); }
    else { rightMin = Math.min(rightMin, x); rightMax = Math.max(rightMax, x); }
  }
  assert.ok(leftMax > leftMin && rightMax > rightMin, "both mixed sources must be present");
  assert.equal(leftMax - leftMin, rightMax - rightMin, "outer Glow must preserve source coverage");
  const shift = rightMin - leftMin, size = 160;
  assert.equal(shift % 2, 0, "paired sources must share the derivative-quad grid");
  const originX = Math.floor((rightMin + rightMax) / 2) - size / 2;
  const originY = image.height / 2 - size / 2;
  const source = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const rgba = (y * size + x) * 4;
    const sample = ((originY + y) * image.width + originX + x - shift) * 4;
    assert.equal(image.data[sample], image.data[sample + 1], "white reference must measure alpha");
    assert.equal(image.data[sample], image.data[sample + 2]);
    source.fill(image.data[sample], rgba, rgba + 4);
  }
  assert.ok(source.some(value => value > 0), "reference must contain the mixed composition");
  const intensity = animated
    ? time < 0.5 ? 1.25 : time < 1.5 ? Math.fround(1.25 + (Math.fround(0.6) - 1.25) * (time - 0.5)) : time < 2.5 ? 0.6 : 0
    : time < 0.5 ? 1.25 : time < 1.5 ? 0.6 : 0;
  const blur = referenceRgbaGaussian(source, size, size, 3.25);
  let maximumError = 0, haloWitness = 0;
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const i = (y * size + x) * 4;
    const alpha = source[i + 3] / 255;
    const halo = (1 - alpha) * Math.min(1, intensity * blur[i + 3]);
    haloWitness = Math.max(haloWitness, Math.round(255 * halo));
    for (let channel = 0; channel < 3; channel++) {
      const expected = Math.round(255 * Math.min(1, alpha + (channel === 0 ? halo : 0)));
      const actual = image.data[((originY + y) * image.width + originX + x) * 4 + channel];
      maximumError = Math.max(maximumError, Math.abs(expected - actual));
    }
  }
  assert.ok(maximumError <= 2, `composed Glow at ${time}: maximum byte error ${maximumError}`);
  if (intensity > 0) assert.ok(haloWitness > 2, "fixture must distinguish filtered output from ordinary drawing");
  else assert.equal(maximumError, 0, "removal must restore exact ordinary mixed content");
  return { channels: size * size * 3, maximumError, haloWitness };
}

await qualifyPairedAuthoring({
  cases: [...[0.25, 0.75, 1.25, 1.75, 2].map(sampleTime => ({
    id: `composed-effects-${sampleTime}`, file: "composed_effects.py", factory: "createDirectComposedEffectsSmokeRenderer",
    duration: 2, sampleTime, playback: "live", canvasSize: [640, 360],
    directHeldSampleTime: sampleTime === 2 ? undefined : Math.floor(sampleTime * 2) / 2,
    boundaries: [0.5, 1, 1.5], stopAtSourceCompletion: sampleTime === 2,
  })), ...[0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2.25, 2.75, 3].map(sampleTime => ({
    id: `composed-animated-${sampleTime}`, file: "composed_effects_animated.py", factory: "createDirectComposedEffectsAnimatedSmokeRenderer",
    duration: 3, sampleTime, playback: "live", canvasSize: [640, 360], animated: true,
    directHeldSampleTime: sampleTime === 0.25 ? 0 : sampleTime === 1.75 ? 1.5 : sampleTime === 2.25 ? 2 : sampleTime === 2.75 ? 2.5 : undefined,
    boundaries: [0.5, 1.5, 2, 2.5], stopAtSourceCompletion: sampleTime === 3,
  }))],
  artifactDirectory: "browser-smoke-artifacts/composed-effects",
  prepareContext: disableAuthoringJspi,
  qualifyPixels({ fixture, rust, python }) {
    return { rust: reference(rust, fixture.sampleTime, fixture.animated), python: reference(python, fixture.sampleTime, fixture.animated) };
  },
});
