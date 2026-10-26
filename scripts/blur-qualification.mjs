import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

await qualifyPairedAuthoring({
  cases: [0.5, 1.5, 2.5, 3.75, 4.5, 5.5, 6].map(sampleTime => ({
    id: `blur-${sampleTime}`, file: "blur.py",
    factory: "createDirectBlurSmokeRenderer", objectCount: 3,
    duration: 6, sampleTime, playback: "live", canvasSize: [640, 360],
    boundaries: [0.75, 2.25, 3, 4.5, 5.25],
    stopAtSourceCompletion: sampleTime === 6,
  })),
  artifactDirectory: "browser-smoke-artifacts/blur-authoring",
  prepareContext: disableAuthoringJspi,
  qualifyPixels({ fixture, rust, python }) {
    if (fixture.sampleTime !== 0.5) return {};
    // At the initial held pose, both images share the same pixels and transform.
    // Ten pixels outside the image extent, only the filtered source contributes.
    for (const png of [rust, python]) {
      const blue = x => png.data[(180 * png.width + x) * 4 + 2];
      assert.ok(blue(458) > blue(282) + 3, "the browser must draw Blur beyond its painted source");
    }
    return { outsideSourceBlurWitness: true };
  },
});
