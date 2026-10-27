// Public Rust/WASM and Python worker programs, with a separately drawn ordinary
// source and independent full-2D operators. No engine intermediate is an oracle.
import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";
import { referenceRgbaGaussian } from "./gaussian-blur-reference.mjs";

const byte = value => Math.max(0, Math.min(255, Math.round(value * 255)));
const gaussian = (source, size, sigma, opacity = 1) =>
  Uint8Array.from(referenceRgbaGaussian(source, size, size, sigma, opacity), byte);
const neutralFrames = new Map();

function glow(source, size, intensity) {
  if (intensity === 0) return source;
  const blur = referenceRgbaGaussian(source, size, size, 3.25);
  const output = new Uint8Array(source.length);
  for (let i = 0; i < source.length; i += 4) {
    const behind = 1 - source[i + 3] / 255;
    const halo = Math.min(1, intensity * blur[i + 3]);
    output[i] = byte(source[i] / 255 + behind * halo);
    output[i + 1] = source[i + 1]; output[i + 2] = source[i + 2];
    output[i + 3] = byte(source[i + 3] / 255 + behind * halo);
  }
  return output;
}

function shadow(source, size, alpha) {
  if (alpha === 0) return source;
  const blur = gaussian(source, size, 2);
  const output = new Uint8Array(source.length);
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const i = (y * size + x) * 4, sx = x - 8, sy = y - 4;
    const mask = sx < 0 || sy < 0 || sx >= size || sy >= size ? 0 : blur[(sy * size + sx) * 4 + 3] / 255;
    const behind = (1 - source[i + 3] / 255) * mask * alpha;
    output[i] = source[i]; output[i + 1] = source[i + 1];
    output[i + 2] = byte(source[i + 2] / 255 + behind);
    output[i + 3] = byte(source[i + 3] / 255 + behind);
  }
  return output;
}

function parameters(time) {
  if (time <= 2) {
    const t = Math.max(0, Math.min(1, time - 0.5));
    return { first: 2 + 2 * t, last: 1 + 2 * t, glow: 1.25, alpha: Math.fround(0.7) };
  }
  const t = Math.max(0, Math.min(1, time - 2));
  return { first: 4 * (1 - t), last: 3 * (1 - t), glow: 1.25 * (1 - t), alpha: Math.fround(Math.fround(0.7) * (1 - t)) };
}

function reference(image, time) {
  let minX = image.width, maxX = -1;
  for (let y = 0; y < image.height; y++) for (let x = 0; x < image.width / 2; x++) {
    if (image.data[(y * image.width + x) * 4] !== 0) { minX = Math.min(minX, x); maxX = Math.max(maxX, x); }
  }
  assert.ok(maxX > minX, "ordinary left source must be present");
  const shift = image.width - 1 - minX - maxX;
  assert.equal(shift % 2, 0, "paired sources must share the derivative-quad grid");
  const size = 160, originX = Math.floor((minX + maxX) / 2 + shift) - size / 2;
  const originY = image.height / 2 - size / 2;
  const source = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const rgba = (y * size + x) * 4;
    const sample = ((originY + y) * image.width + originX + x - shift) * 4;
    assert.equal(image.data[sample], image.data[sample + 1]);
    assert.equal(image.data[sample], image.data[sample + 2]);
    source.fill(image.data[sample], rgba, rgba + 4); // white RGB on black measures coverage
  }
  const p = parameters(time);
  const first = gaussian(source, size, p.first);
  const expected = gaussian(shadow(glow(first, size, p.glow), size, p.alpha), size, p.last, 0.5);
  const reversed = time < 3 ? gaussian(glow(shadow(first, size, p.alpha), size, p.glow), size, p.last, 0.5) : null;
  let maximumError = 0, orderWitness = 0;
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) for (let channel = 0; channel < 3; channel++) {
    const i = (y * size + x) * 4 + channel;
    const actual = image.data[((originY + y) * image.width + originX + x) * 4 + channel];
    maximumError = Math.max(maximumError, Math.abs(expected[i] - actual));
    if (reversed) orderWitness = Math.max(orderWitness, Math.abs(expected[i] - reversed[i]));
  }
  assert.ok(maximumError <= 2, `four ordered passes at ${time}: maximum byte error ${maximumError}`);
  if (time <= 2) assert.ok(orderWitness > 2, "ordinary source must distinguish Glow/Shadow order");
  return { channels: size * size * 3, maximumError, orderWitness };
}

await qualifyPairedAuthoring({
  cases: [0.25, 0.5, 0.75, 1, 1.5, 1.75, 2, 2.25, 2.5, 3, 3.25, 3.5, 3.75, 4].map(sampleTime => ({
    id: `ordered-effects-${sampleTime}`, file: "ordered_effects.py", factory: "createDirectOrderedEffectsSmokeRenderer",
    objectCount: 3, duration: 4, sampleTime, playback: "live", canvasSize: [640, 360],
    directHeldSampleTime: sampleTime === 0.25 ? 0 : sampleTime === 1.75 ? 1.5 : sampleTime === 3.25 ? 3 : sampleTime === 3.75 ? 3.5 : undefined,
    boundaries: [0.5, 1.5, 2, 3, 3.5], stopAtSourceCompletion: sampleTime === 4,
  })),
  artifactDirectory: "browser-smoke-artifacts/ordered-effects",
  prepareContext: disableAuthoringJspi,
  qualifyPixels({ fixture, backend, rust, python }) {
    const result = { rust: reference(rust, fixture.sampleTime), python: reference(python, fixture.sampleTime) };
    if (fixture.sampleTime >= 3) {
      for (const [host, image] of [["rust",rust],["python",python]]) {
        const key = `${backend}:${host}`;
        if (neutralFrames.has(key)) assert.deepEqual(image.data,neutralFrames.get(key),"neutral and removal must preserve the exact ordinary image");
        else neutralFrames.set(key,Buffer.from(image.data));
      }
      result.exactNeutralAndRemoval = true;
    }
    return result;
  },
});
