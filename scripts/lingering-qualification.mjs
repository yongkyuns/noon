import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import assert from "node:assert/strict";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_LINGERING_ARTIFACTS ?? "lingering-artifacts",
  cases: [{ id: "lingering", sourcePath: "parity/manim-v0.21/core-examples/lingering.py",
    scene: "LingeringRate", factory: "createDirectLingeringRenderer",
    objectCount: 2, duration: 1, sampleTime: 0.6, playback: "live" }],
  qualifyPixels({ rust }) {
    let visible = 0;
    for (let i = 0; i < rust.data.length; i += 4) {
      if (rust.data[i] + rust.data[i + 1] + rust.data[i + 2] > 50) visible++;
    }
    assert.ok(visible > 100, "the original rate tracks must be visible");
    const scale = rust.height / 8;
    // Unchanged Manim lingering(0.6) is 0.75: x=-2+4*0.75=1.
    const x = Math.round(rust.width / 2 + scale);
    const y = Math.round(rust.height / 2 - 2 * scale);
    const offset = (y * rust.width + x) * 4;
    assert.deepEqual([...rust.data.subarray(offset, offset + 3)], [255, 0, 0]);
    return { visible, originalForwardPosition: 1 };
  },
});
