import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

const wideFrames = new Map();
const scene = { file: "zoomed_scene.py", factory: "createZoomedSceneRenderer", objectCount: 5, duration: 1 };
await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_ZOOM_ARTIFACTS ?? "zoom-artifacts",
  cases: [
    ...[0, 0.5, 1].map(sampleTime => ({ ...scene, id: `zoomed-scene-${sampleTime}`, sampleTime })),
    ...[800, 360].map(width => ({
      ...scene, id: `zoomed-scene-width-${width}`, sampleTime: 1, canvasSize: [width, 540],
    })),
  ],
  qualifyPixels({ fixture, backend, rust }) {
    if (fixture.id === "zoomed-scene-1") wideFrames.set(backend, rust);
    if (!fixture.canvasSize) return {};
    const wide = wideFrames.get(backend);
    assert.ok(wide, "viewport qualification requires the full-width reference case");
    const offset = (wide.width - rust.width) / 2;
    // Equal height preserves world-to-pixel scale. Narrowing the viewport must
    // clip the unchanged scene, not move the inset or crop its source camera.
    let differingPixels = 0;
    for (let y = 0; y < rust.height; y++) {
      for (let x = 0; x < rust.width; x++) {
        const actual = (y * rust.width + x) * 4;
        const expected = (y * wide.width + x + offset) * 4;
        if (!rust.data.subarray(actual, actual + 4).equals(wide.data.subarray(expected, expected + 4))) differingPixels++;
      }
    }
    assert.equal(differingPixels, 0, "narrow viewport must equal the centered crop of the full scene");
    return { centeredCropDifferingPixels: differingPixels };
  },
});
