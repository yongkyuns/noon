import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const canonical = await readFile(
  new URL("../parity/manim-v0.21/stress-examples/mixed_object_parity_stress.py", import.meta.url),
  "utf8",
);
const noon = await readFile(
  new URL("./python/examples/manim_parity_stress_grid.py", import.meta.url),
  "utf8",
);
const retainedAnimate = await readFile(
  new URL("./python/_manim_retained_animate.py", import.meta.url),
  "utf8",
);
const familyCreation = await readFile(
  new URL("./python/_manim_family_creation.py", import.meta.url),
  "utf8",
);
const browserSmoke = await readFile(new URL("./manim-compat-smoke.html", import.meta.url), "utf8");

assert.equal(
  noon,
  canonical.replace("from manim import *", "from noon import *"),
  "stress source must remain import-only Manim compatible",
);
assert.match(noon, /rows = 20/);
assert.match(noon, /cols = 30/);
assert.match(noon, /shape_count = rows \* cols/);
assert.match(noon, /self\.play\(FadeIn\(title\), FadeIn\(subtitle\), run_time=0\.25\)/);
assert.match(noon, /self\.play\(\*\[Create\(shape\) for shape in shapes\], run_time=0\.55\)/);
assert.doesNotMatch(
  noon,
  /Create\(title\)/,
  "retained family Create cannot share the stress scene's mass mixed-animation play yet",
);
assert.match(noon, /for shape, target in zip\(shapes, targets_a\)/);
assert.match(noon, /for shape, target in zip\(shapes, targets_b\)/);
assert.match(noon, /motion_a = \[\]/);
assert.match(noon, /motion_b = \[\]/);
assert.match(noon, /leaving = shapes\[::3\]/);
assert.match(noon, /title\.animate\.rotate\(PI \/ 24\)/);
assert.match(noon, /title\.animate\.rotate\(-PI \/ 12\)/);
assert.doesNotMatch(
  noon,
  /(?:title|subtitle)\.animate[^\n]*set_color/,
  "public stress scene must not claim retained Text color animation before a color track exists",
);
assert.match(
  retainedAnimate,
  /position, rotation, opacity, and uniform scale animations are supported/,
  "contract should track the retained Text animation capability boundary",
);
assert.match(
  retainedAnimate,
  /mixing retained Text animations with legacy animations in one Scene\.play /,
  "contract should track the retained-vs-geometry mixed-play capability boundary",
);
assert.match(
  familyCreation,
  /canonical retained family creation animation must currently be the only animation in Scene\.play/,
  "contract should track the retained family mixed-play capability boundary",
);
assert.match(browserSmoke, /manim_parity_stress_grid\.py/);
assert.match(browserSmoke, /stressResult\.document\.objects\.length !== 800/);
assert.match(browserSmoke, /stressResult\.sceneSpec\.objects\.length !== 802/);
assert.match(browserSmoke, /expectedObjectCount: 602/);
assert.match(browserSmoke, /waitForRenderedState/);
assert.match(browserSmoke, /seekTime: 2\.8/);

console.log("✓ stress runtime capability and browser-execution contract");
