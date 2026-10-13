import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import assert from "node:assert/strict";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_CUBIC_BEZIER_ARTIFACTS ?? "cubic-bezier-artifacts",
  cases: [{ id: "cubic-bezier", sourcePath: "parity/manim-v0.21/core-examples/cubic_bezier.py",
    scene: "CubicBezierConstructor", factory: "createDirectCubicBezierSmokeRenderer",
    objectCount: 1, duration: 0.2, sampleTime: 0.2, playback: "live" }],
  qualifyPixels({ rust }) {
    let visible = 0;
    for (let index = 0; index < rust.data.length; index += 4) {
      if (rust.data[index] + rust.data[index + 1] + rust.data[index + 2] > 50) visible += 1;
    }
    assert.ok(visible > 100, "the paired cubic curve must have visible pixels");
    return { visible };
  },
});
