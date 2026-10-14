import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const reference = await readFile(new URL("../crates/noon-core/src/animation/polynomial_easing_reference.csv", import.meta.url), "utf8");
const quarterSamples = reference.split("\n").filter(row => !row.startsWith("#"))
  .map(row => row.split(",")).filter(row => Number(row[1]) === 0.25);
assert.equal(quarterSamples.length, 12);

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
    const colors = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [0, 255, 255], [255, 0, 255]];
    const scale = rust.height / 8;
    for (const [index, sample] of quarterSamples.entries()) {
      const x = Math.round(rust.width / 2 + (-2 + 4 * Number(sample[2])) * scale);
      const y = Math.round(rust.height / 2 - (2 - index * 0.38) * scale);
      const offset = (y * rust.width + x) * 4;
      assert.deepEqual([...rust.data.subarray(offset, offset + 3)], colors[index % colors.length],
        `${sample[0]} must reach its original Manim quarter-time position`);
    }
    return { visible, originalQuarterPositions: quarterSamples.length };
  },
});
