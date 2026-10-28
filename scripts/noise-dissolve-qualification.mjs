// Ordinary coverage is measured from the unfiltered left composition. Mask math
// is evaluated in double precision without engine uniforms, pixels or helpers.
import assert from "node:assert/strict";
import { dissolveCoverage } from "./dissolve-mask-reference.mjs";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

const mix = (a, b, t) => a * (1 - t) + b * t;
function parameters(time) {
  if (time < 0.5 || time >= 4) return { seed: 7, softness: 0.08, progress: 0 };
  if (time <= 1.5) return { seed: 7, softness: 0.08, progress: time - 0.5 };
  if (time <= 2) return { seed: 7, softness: 0.08, progress: 1 };
  if (time < 3) return { seed: 7, softness: mix(0.08, 0.4, time - 2), progress: 3 - time };
  return { seed: time < 3.5 ? 7 : 4294967295, softness: 0.4, progress: time < 3.5 ? 0 : 0.5 };
}
function reference(image, time) {
  assert.equal(image.width, 640); assert.equal(image.height, 360);
  const p = parameters(time);
  const scale = image.height / 8; // default camera height, independent of the background size
  const shift = 4 * scale;
  const min = [image.width / 2 + 1.2 * scale, image.height / 2 - 0.4 * scale];
  let maximumError = 0, channels = 0, perLeafWitness = 0, sourceWitness = 0;
  // Source bounds are independently specified by the authored two rectangles:
  // world [1.2,2.8] x [-.4,.4], viewed through the default height-eight camera.
  for (let y = 140; y < 220; y++) for (let x = 344; x < 456; x++) {
    const source = image.data[(y * image.width + x - shift) * 4];
    sourceWitness = Math.max(sourceWitness, source);
    const mask = dissolveCoverage((x + 0.5 - min[0]) / (1.6 * scale), (y + 0.5 - min[1]) / (0.8 * scale), p);
    const expected = Math.round(source * mask);
    for (let c = 0; c < 3; c++) {
      const actual = image.data[(y * image.width + x) * 4 + c];
      maximumError = Math.max(maximumError, Math.abs(expected - actual)); channels++;
    }
    if (source >= 210 && source <= 216) {
      const perLeaf = Math.round(255 * (1 - (1 - 0.6 * mask) ** 2));
      perLeafWitness = Math.max(perLeafWitness, Math.abs(expected - perLeaf));
    }
  }
  assert.ok(sourceWitness > 200, "ordinary source must contain overlapping leaves");
  assert.ok(maximumError <= 2, `mask at ${time}: maximum byte error ${maximumError}`);
  if (p.progress === 0 || p.progress === 1) assert.equal(maximumError, 0, "mask endpoints are exact");
  else assert.ok(perLeafWitness > 3, "fixture must distinguish one composed mask from masks on each leaf");
  return { channels, maximumError, perLeafWitness, ...p };
}

await qualifyPairedAuthoring({
  cases: [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2, 2.25, 2.5, 3, 3.25, 3.5, 3.75, 4, 4.25, 4.5].map(sampleTime => ({
    id: `noise-dissolve-${sampleTime}`, file: "noise_dissolve.py", factory: "createDirectNoiseDissolveSmokeRenderer",
    duration: 4.5, sampleTime, playback: "live", canvasSize: [640, 360],
    directHeldSampleTime: sampleTime === 0.25 ? 0 : sampleTime === 1.75 ? 1.5 : sampleTime === 3.25 ? 3 : sampleTime === 3.75 ? 3.5 : sampleTime === 4.25 ? 4 : undefined,
    boundaries: [0.5, 1.5, 2, 3, 3.5, 4], stopAtSourceCompletion: sampleTime === 4.5,
  })),
  artifactDirectory: "browser-smoke-artifacts/noise-dissolve",
  prepareContext: disableAuthoringJspi,
  qualifyPixels({ fixture, rust, python }) {
    return { rust: reference(rust, fixture.sampleTime), python: reference(python, fixture.sampleTime) };
  },
});
