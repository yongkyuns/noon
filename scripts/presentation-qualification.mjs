// The same themed scene is authored directly in Rust/WASM and through the
// production Python worker. The shared harness compares actual pixels exactly.
import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_PRESENTATION_ARTIFACTS ?? "presentation-artifacts",
  cases: [{
    id: "presentation-presets", file: "presentation_presets.py", scene: "PresentationPresets",
    factory: "createDirectPresentationPresetsSmokeRenderer", objectCount: 26,
    duration: 0, sampleTime: 0,
  }],
  qualifyPixels({ rust }) {
    // Distinct panels must both be present; exact frontend equality alone
    // would accept two equally blank images.
    const pixel = (x, y) => {
      const offset = (Math.floor(y) * rust.width + Math.floor(x)) * 4;
      return [...rust.data.subarray(offset, offset + 3)];
    };
    const scale = rust.height / 8;
    const left = pixel(rust.width / 2 - 3.35 * scale, rust.height / 2 - 2.5 * scale);
    const right = pixel(rust.width / 2 + 3.35 * scale, rust.height / 2 - 2.5 * scale);
    assert.ok(left.reduce((sum, value) => sum + value, 0) < 150, "dark theme panel must be visible");
    assert.ok(right.every(value => value > 220), "light theme panel must be visible");
    return { darkPanel: left, lightPanel: right };
  },
});
