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
const sourceOwnedStress = await readFile(
  new URL("../scripts/playground-stress-edit-smoke.mjs", import.meta.url),
  "utf8",
);

assert.equal(
  noon,
  canonical.replace("from manim import *", "from noon import *"),
  "stress source must remain import-only source compatible",
);
assert.match(noon, /class MixedObjectParityStress\(Scene\)/);
assert.match(noon, /rows = 20/);
assert.match(noon, /cols = 30/);
assert.match(noon, /shape_count = rows \* cols/);
assert.match(noon, /for index in range\(24\)/);
assert.match(noon, /\*\[FadeIn\(label\) for label in labels\]/);
assert.match(noon, /for wave in range\(6\)/);
assert.match(noon, /bucket = \(index \* 37 \+ row \* 11 \+ col \* 7\) % 6/);
assert.match(noon, /shape\.animate\.scale\(scale_factor\)/);
assert.match(noon, /\.rotate\(angle\)/);
assert.match(noon, /\.shift\(dx \* RIGHT \+ dy \* UP\)/);
assert.match(noon, /\.set_color\(color\)/);
assert.match(noon, /label\.animate\.scale\(factor\)/);
assert.match(noon, /\.set_opacity\(opacity\)/);
assert.match(noon, /\.set_opacity\(1\.0\)/);
assert.match(noon, /turbulence = \[\]/);
assert.match(noon, /leaving = shapes\[::3\]/);
assert.match(noon, /blinking_labels = labels\[::3\]/);
assert.doesNotMatch(noon, /MANIM/i, "public stress scene copy must not expose compatibility branding");
assert.doesNotMatch(
  noon,
  /(?:title|subtitle|label)\.animate[^\n]*set_color/,
  "retained Text color animation must not be claimed before a color track exists",
);
assert.match(
  retainedAnimate,
  /position, rotation, opacity, and uniform scale animations are supported/,
  "contract should track the retained Text animation capability boundary",
);
assert.match(
  retainedAnimate,
  /mixing retained Text animations with legacy animations in one Scene\.play /,
  "standalone retained-vs-geometry property animation remains a separate composition boundary",
);
assert.doesNotMatch(
  familyCreation,
  /must currently be the only animation in Scene\.play/,
  "disjoint retained family animations should no longer be artificially single-animation-only",
);
assert.doesNotMatch(
  familyCreation,
  /retained Text property animations in the same Scene\.play still require/,
  "family composition should now admit direct retained Text property animations",
);
assert.match(
  familyCreation,
  /_retained\._schedule_retained_plan/,
  "family composition must reuse the retained property-track scheduler",
);
assert.match(
  familyCreation,
  /concurrent retained family animations must target disjoint family leaves/,
  "contract should reject ambiguous same-leaf concurrent family ownership",
);
assert.match(
  familyCreation,
  /concurrent retained family and ordinary animations must target[\s\S]*disjoint scene leaves/,
  "contract should reject family-vs-ordinary same-leaf ownership",
);
assert.doesNotMatch(
  browserSmoke,
  /manim_parity_stress_grid\.py|retained-stress-smoke/,
  "canonical Text stress must not re-enter the legacy SceneSpec export fixture",
);
assert.match(sourceOwnedStress, /example=manim-parity-stress-grid/);
assert.match(sourceOwnedStress, /selectedExampleId === "manim-parity-stress-grid"/);
assert.match(sourceOwnedStress, /const source = await page\.evaluate/);
assert.match(sourceOwnedStress, /assert\.match\(source, \/rows = 20\//);
assert.match(sourceOwnedStress, /for \(const rows of \[5, 7, 20\]\)/);
assert.match(sourceOwnedStress, /Scene rebuilt atomically/);

console.log("✓ stress runtime capability and browser-execution contract");
