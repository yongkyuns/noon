import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

await qualifyPairedAuthoring({
  cases: [0.5, 1.5, 2.5, 3.75, 4.5, 5.5, 6].map(sampleTime => ({
    id: `shadow-${sampleTime}`, file: "shadow.py",
    factory: "createDirectShadowSmokeRenderer", objectCount: 3,
    duration: 6, sampleTime, playback: "live", canvasSize: [640, 360],
    directHeldSampleTime: sampleTime === 0.5 ? 0 : sampleTime === 2.5 ? 2.25
      : sampleTime === 5.5 ? 5.25 : undefined,
    boundaries: [0.75, 2.25, 3, 4.5, 5.25],
    stopAtSourceCompletion: sampleTime === 6,
  })),
  artifactDirectory: "browser-smoke-artifacts/shadow-authoring",
  prepareContext: disableAuthoringJspi,
  qualifyPixels({ fixture, rust, python }) {
    if (fixture.sampleTime !== 0.5) return {};
    for (const png of [rust, python]) {
      const blue = x => png.data[(190 * png.width + x) * 4 + 2];
      assert.ok(blue(465) < blue(282) - 3, "offset Shadow must darken pixels outside its source");
    }
    return { outsideSourceShadowWitness: true };
  },
});
