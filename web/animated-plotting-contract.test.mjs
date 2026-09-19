import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { animatedPlottingCases, animatedPresentedTime } from "../scripts/animated-plotting-checks.mjs";
const read = name => readFile(new URL(name, import.meta.url), "utf8");

test("static qualification cannot accidentally consume an animated gallery source", async () => {
  const harness = await read("../scripts/plotting-qualification.mjs");
  assert.match(harness, /examples\/coordinate_plotting_static\.py/);
  const source = await read("./python/examples/coordinate_plotting_static.py");
  assert.doesNotMatch(source, /self\.(?:play|wait)\(/);
  assert.match(source, /self\.add\(axes, curve, data, title, label\)/);
});

test("both public animations run via a real continuation consumer in the paired gate", async () => {
  const harness = await read("../scripts/time-series-plotting-qualification.mjs");
  const workflow = await read("../.github/workflows/manim-plotting-qualification.yml");
  assert.match(harness, /onSemanticContinuation\(registration\)/);
  assert.match(harness, /sampleToAuthoredTime\(time\)/);
  for (const mode of Object.keys(animatedPlottingCases)) {
    assert.ok(workflow.includes(`time-series-plotting-qualification.mjs ${mode}`));
  }
});

test("quiet waits only retain their last visual timestamp inside declared wait intervals", () => {
  assert.equal(animatedPresentedTime(2.3, "coordinates"), 2.3);
  assert.equal(animatedPresentedTime(4.7, "coordinates"), 4.3);
  assert.equal(animatedPresentedTime(5.1, "coordinates"), 4.3);
  assert.equal(animatedPresentedTime(3.75, "number-line"), 3.5);
  assert.equal(animatedPresentedTime(4, "number-line"), 4);
  assert.equal(animatedPresentedTime(5.7, "number-line"), 5.7);
  assert.equal(animatedPresentedTime(6.1, "number-line"), 6.1);
  assert.equal(animatedPresentedTime(8.8, "number-line"), 8);
  for (const invalid of [NaN, -1, Infinity, 9]) {
    assert.throws(() => animatedPresentedTime(invalid, "number-line"));
  }
});

test("browser sampling imports only the portable fixture policy", async () => {
  const policy = await read("../scripts/animated-plotting-samples.mjs");
  assert.doesNotMatch(policy, /node:/);
  const harness = await read("../scripts/time-series-plotting-qualification.mjs");
  assert.match(harness, /await import\("\.\.\/scripts\/animated-plotting-samples\.mjs"\)/);
  assert.doesNotMatch(harness, /await import\("\.\.\/scripts\/animated-plotting-checks\.mjs"\)/);
});
