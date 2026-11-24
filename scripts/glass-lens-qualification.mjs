// Normal Rust/WASM and Pyodide-worker authoring on both portable GPU backends.
import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";
const images = new Map();
const times = [0.25, 0.5, 1.25, 2, 2.25, 2.5, 3.25, 4, 4.25, 4.5, 4.75, 5, 5.25, 5.5, 6, 6.5, 6.75, 7, 7.5, 8, 8.25, 8.5];
const held = new Map([[0.25, 0], [2.25, 2], [4.25, 4], [4.75, 4.5], [5.25, 5], [6.75, 6.5], [8.25, 8]]);
function prefix(image) {
  let maximumByteError = 0, channels = 0;
  for (let y = 0; y < 180; y++) for (let x = 0; x < 320; x++) {
    if ([39, 40, 279, 280].includes(x) || [29, 30, 149, 150].includes(y)) continue;
    const expected = x >= 40 && x < 280 && y >= 30 && y < 150
      ? ((Math.floor((x - 40) / 15) + Math.floor((y - 30) / 15)) % 2 === 0 ? [28, 82, 148, 255] : [240, 166, 70, 255])
      : [8, 16, 31, 255];
    for (let c = 0; c < 4; c++) {
      const error = Math.abs(image.data[(y * 320 + x) * 4 + c] - expected[c]);
      maximumByteError = Math.max(maximumByteError, error); channels++;
      assert.ok(error <= 2, `full-out literal prefix at ${x},${y}/${c}: ${error}`);
    }
  }
  return { maximumByteError, channels };
}
await qualifyPairedAuthoring({
  cases: times.map(sampleTime => ({
    id: `glass-${sampleTime}`, file: "showcase_glass_lens.py", factory: "createDirectGlassLensSmokeRenderer",
    duration: 8.5, sampleTime, playback: "live", canvasSize: [320, 180],
    directHeldSampleTime: held.get(sampleTime),
    boundaries: [0.5, 2, 2.5, 4, 4.5, 5, 5.5, 6.5, 7, 8],
    stopAtSourceCompletion: sampleTime === 8.5,
  })),
  artifactDirectory: "browser-smoke-artifacts/glass-lens", prepareContext: disableAuthoringJspi,
  qualifyPixels({ fixture, rust, python, backend }) {
    const result = {};
    // The harness compares every Rust/Python pixel. These extra witnesses check
    // observable lifecycle meaning independently of frontend equality.
    for (const [host, image] of [["rust", rust], ["python", python]]) {
      const key = `${backend}/${host}`;
      const previous = images.get(key) ?? new Map(); images.set(key, previous);
      previous.set(fixture.sampleTime, Buffer.from(image.data));
      if (fixture.sampleTime === 4.75) assert.deepEqual(image.data, previous.get(4.25), "zero radius and removal must agree exactly");
      if (fixture.sampleTime === 8.25) assert.deepEqual(image.data, previous.get(5.25), "visibility must restore the glass exactly");
      if (fixture.sampleTime === 6.75) result[host] = prefix(image);
    }
    return result;
  },
});
