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
  browserSmoke,
  /manim_parity_stress_grid\.py|retained-stress-smoke/,
  "canonical Text stress must not re-enter the legacy document export fixture",
);
assert.match(sourceOwnedStress, /example=manim-parity-stress-grid/);
assert.match(sourceOwnedStress, /selectedExampleId === "manim-parity-stress-grid"/);
assert.match(sourceOwnedStress, /const source = await page\.evaluate/);
assert.match(sourceOwnedStress, /assert\.match\(source, \/rows = 20\//);
assert.match(sourceOwnedStress, /const rows5Source = source\s*\.replace/);
assert.match(sourceOwnedStress, /const rows7Source = source\.replace\(\/rows = \\d\+\/, "rows = 7"\)/);
assert.match(sourceOwnedStress, /const rows20Source = rows7Source\.replace\(\/rows = \\d\+\/, "rows = 20"\)/);
assert.match(sourceOwnedStress, /const SOURCE_OWNED_DURATION_SECONDS = 600;/);
assert.match(sourceOwnedStress, /const firstStressPlayBoundary =/);
assert.match(sourceOwnedStress, /self\.wait\(\$\{SOURCE_OWNED_DURATION_SECONDS\}\.0\)/);
const rows5Draft = sourceOwnedStress.indexOf("rows5Draft = await replaceSource(editor, page, rows5Source)");
const rows5Run = sourceOwnedStress.indexOf('await page.locator("#replace-scene").click()', rows5Draft);
const rows5Playing = sourceOwnedStress.indexOf("rows5Playing = await waitForLongSourcePlayback", rows5Run);
const rows7Draft = sourceOwnedStress.indexOf("rows7EditedDuringRows5 = await replaceSource(editor, page, rows7Source)", rows5Playing);
const rows7Run = sourceOwnedStress.indexOf('await page.locator("#replace-scene").click()', rows7Draft);
const rows7Apply = sourceOwnedStress.indexOf("rows7Rerun = await waitForAppliedRun(", rows7Run);
const rows20Draft = sourceOwnedStress.indexOf("rows20Edited = await replaceSource(editor, page, rows20Source)", rows7Apply);
const rows20Run = sourceOwnedStress.indexOf("await page.keyboard.press(runShortcut)", rows20Draft);
const rows20Apply = sourceOwnedStress.indexOf("rows20Rerun = await waitForAppliedRun(", rows20Run);
assert.ok(rows5Draft >= 0 && rows5Run > rows5Draft && rows5Playing > rows5Run);
assert.ok(rows7Draft > rows5Playing && rows7Run > rows7Draft && rows7Apply > rows7Run);
assert.ok(rows20Draft > rows7Apply && rows20Run > rows20Draft && rows20Apply > rows20Run);
const draftOnlyInterval = sourceOwnedStress.slice(rows5Playing, rows7Draft);
assert.match(draftOnlyInterval, /snapshot\(page, \{ reset: true \}\)/);
assert.match(draftOnlyInterval, /assertDraftOnlyEdit/);
assert.match(draftOnlyInterval, /generation: sourceOwnedGeneration/);
assert.doesNotMatch(draftOnlyInterval, /replace-scene.*click|keyboard\.press\(runShortcut\)/);
assert.match(sourceOwnedStress, /const rows7Generation = sourceOwnedGeneration \+ 2/);
assert.match(sourceOwnedStress, /rows20Generation|rows7Generation \+ 1/);
assert.match(sourceOwnedStress, /patchRunGeneration/);
assert.match(sourceOwnedStress, /draftState, "unapplied"/);
assert.match(sourceOwnedStress, /draftState, "applied"/);

console.log("✓ stress runtime capability and browser-execution contract");
