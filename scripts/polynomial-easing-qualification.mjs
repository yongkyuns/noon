import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import assert from "node:assert/strict";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_POLYNOMIAL_EASING_ARTIFACTS ?? "polynomial-easing-artifacts",
  cases: [{ id: "polynomial-easings", sourcePath: "parity/manim-v0.21/core-examples/polynomial_easings.py",
    scene: "PolynomialEasings", factory: "createDirectPolynomialEasingsRenderer",
    objectCount: 12, duration: 1, sampleTime: 0.25, playback: "live" }],
  qualifyPixels({ rust }) {
    let visible = 0;
    for (let i = 0; i < rust.data.length; i += 4) {
      if (rust.data[i] + rust.data[i+1] + rust.data[i+2] > 50) visible++;
    }
    assert.ok(visible > 100, "the easing tracks must be visible");
    return { visible };
  },
});
