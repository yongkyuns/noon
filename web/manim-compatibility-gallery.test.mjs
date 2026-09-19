import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";

import { normalizeGalleryManifest } from "./example-gallery.js";

const manifest = JSON.parse(
  await readFile(
    new URL("./python/examples/manim_compatibility_manifest.json", import.meta.url),
    "utf8",
  ),
);
const readyEntries = manifest.entries.filter((entry) => entry.status === "ready");
const gallery = normalizeGalleryManifest(manifest);

assert.equal(manifest.reference.version, "0.21.0");
assert.equal(gallery.examples.length, 24);
assert.deepEqual(
  gallery.examples.map((entry) => entry.id),
  [
    "compatible-affine-fade",
    "compatible-family-transform-indicate",
    "compatible-timed-composition",
    "compatible-family-write",
    "compatible-subset-display",
    "compatible-scale-in-place",
    "compatible-indicate-square",
    "compatible-affine-lifecycle",
    "compatible-text-write",
    "compatible-text-family-fade",
    "compatible-text-family-reveal",
    "compatible-group-slicing",
    "compatible-arrow-vector-field-static",
    "compatible-svg-tiger-morph",
    "noon-trigonometry-tutorial",
    "noon-transform-matching-shapes",
    "noon-transform-matching-shapes-breadth",
    "noon-coordinate-plotting",
    "noon-number-plane",
    "noon-implicit-plotting",
    "noon-area-helpers",
    "noon-raster-image",
    "noon-markup-text",
    "noon-text-range-colors",
  ],
);

const noonOnlyPatterns = [
  /self\.value_tracker\(/,
  /self\.bind_position\(/,
  /async\s+def\s+construct\(/,
  /await\s+self\./,
  /\._scene\b/,
];

for (const entry of readyEntries) {
  assert.equal(
    entry.reuse,
    "manim-compatible-parity-v0.21",
    `${entry.id}: authored compatibility examples must use the compatibility reuse class`,
  );
  assert.ok(
    entry.category.startsWith("manim-compatible/"),
    `${entry.id}: category must visibly identify the compatibility contract`,
  );
  assert.ok(
    entry.features.includes("Manim-compatible"),
    `${entry.id}: compatibility must remain searchable from the gallery`,
  );

  await access(new URL(`./${entry.path}`, import.meta.url));
  if (!entry.thumbnail.startsWith("https://")) {
    await access(new URL(`./${entry.thumbnail}`, import.meta.url));
  }

  const source = await readFile(new URL(`./${entry.path}`, import.meta.url), "utf8");
  assert.match(source, /from noon import\b/, `${entry.id}: public source must import Noon`);
  for (const pattern of noonOnlyPatterns) {
    assert.doesNotMatch(
      source,
      pattern,
      `${entry.id}: Manim-compatible source must not depend on Noon-only helper ${pattern}`,
    );
  }

  if (entry.id === "noon-text-range-colors") {
    assert.match(source, /Text\(/, "Text range gallery example must construct Text");
    assert.match(source, /t2c=|text2color=/, "Text range gallery example must exercise t2c/text2color");
    assert.match(source, /\[6:10\]/, "Text range gallery example must exercise a positive source slice");
    assert.match(source, /\[-1:\]/, "Text range gallery example must exercise a negative source slice");
    assert.match(source, /café|Ω/, "Text range gallery example must exercise Unicode");
    assert.match(source, /\\n/, "Text range gallery example must exercise multiline text");
    assert.match(source, /DejaVu Sans Mono/, "Text range gallery example must pin its font");
    assert.equal(entry.parity_fixture, "text-range-colors");
  }

  if (entry.id === "noon-markup-text") {
    assert.match(source, /MarkupText\(/, "MarkupText gallery example must construct MarkupText");
    assert.match(source, /<b>/, "MarkupText gallery example must exercise bold markup");
    assert.match(source, /<i>/, "MarkupText gallery example must exercise italic markup");
    assert.match(source, /<tt>/, "MarkupText gallery example must exercise monospace markup");
    assert.match(source, /<span foreground=/, "MarkupText gallery example must exercise foreground spans");
    assert.match(source, /\\n/, "MarkupText gallery example must exercise multiline text");
    assert.match(source, /DejaVu Sans Mono/, "MarkupText gallery example must pin its font");
  }

  if (entry.id === "compatible-text-write") {
    assert.match(source, /Text\(/, "plain Text Write example must construct Text");
    assert.match(source, /Write\(/, "plain Text Write example must exercise Write");
    assert.match(source, /Unwrite\(/, "plain Text Write example must exercise Unwrite");
    assert.doesNotMatch(
      source,
      /\b(?:VGroup|Group|Typst|MathTypst)\b/,
      "plain Text Write gallery coverage must not claim Text-family or Typst scheduling",
    );
  }

  if (entry.id === "noon-transform-matching-shapes") {
    assert.match(source, /TransformMatchingShapes\(/, "matching-shapes gallery example must exercise TransformMatchingShapes");
    assert.match(source, /Indicate\(/, "matching-shapes gallery example must exercise the replacement target");
    assert.match(source, /readded=True/, "matching-shapes gallery example must document source re-add semantics");
    assert.match(entry.summary, /shape-keyed/i, "matching-shapes gallery must state its pairing contract");
  }

  if (entry.id === "noon-transform-matching-shapes-breadth") {
    assert.match(source, /source_first|source_second|target_padded/, "matching-shapes breadth must cover duplicate-key growth");
    assert.match(source, /rotated_triangle|target_leftover/, "matching-shapes breadth must cover an unmatched target");
    assert.match(source, /run_time=1\.0/, "matching-shapes breadth must use one-second transform/indicate timings");
    assert.match(source, /roots = self\.mobjects/, "matching-shapes breadth must assert target roots after Indicate");
    assert.match(source, /tuple\(target\.submobjects\)/, "matching-shapes breadth must assert target family members");
    assert.match(source, /Indicate\(/, "matching-shapes breadth must retain the following Indicate");
    assert.match(entry.summary, /duplicate-key/i, "matching-shapes breadth must be marked as duplicate-key coverage");
  }

  if (entry.id === "compatible-text-family-fade") {
    assert.match(source, /VGroup\(/, "Text family fade example must construct a VGroup");
    assert.match(source, /FadeIn\(/, "Text family fade example must exercise FadeIn");
    assert.match(source, /FadeOut\(/, "Text family fade example must exercise FadeOut");
    assert.match(source, /Write\(/, "Text family fade example must compose with Write");
    assert.match(source, /lag_ratio\s*=\s*0\.25/, "Text family fade must exercise lagged family timing");
    assert.doesNotMatch(
      source,
      /\b(?:Typst|MathTypst)\b/,
      "Text family fade gallery coverage must not claim deferred Typst family scheduling",
    );
  }

  if (entry.id === "compatible-text-family-reveal") {
    assert.match(source, /VGroup\(/, "Text family reveal example must construct a VGroup");
    assert.match(source, /Create\(/, "Text family reveal example must exercise Create");
    assert.match(source, /Uncreate\(/, "Text family reveal example must exercise Uncreate");
    assert.match(source, /lag_ratio\s*=\s*0\.25/, "Text family reveal must exercise global glyph timing");
    assert.doesNotMatch(
      source,
      /\b(?:Typst|MathTypst)\b/,
      "Text family reveal gallery coverage must not claim deferred Typst family scheduling",
    );
  }
}

const slicingEntry = readyEntries.find((entry) => entry.id === "compatible-group-slicing");
assert.ok(slicingEntry, "group slicing must be a ready compatibility example");
assert.equal(slicingEntry.parity_fixture, "group-slicing");
const slicingSource = await readFile(new URL(`./${slicingEntry.path}`, import.meta.url), "utf8");
assert.match(
  slicingSource,
  /self\.play\(family\[1:\]\.animate\(run_time=1\.0\)\.shift\(UP \* 0\.75\)\)/,
  "group slicing gallery example must visibly animate the selected shared members",
);
const slicingCanonical = await readFile(
  new URL("../parity/manim-v0.21/core-examples/group_slicing.py", import.meta.url),
  "utf8",
);
assert.equal(
  slicingSource.replace("from noon import *", "from manim import *"),
  slicingCanonical,
  "group slicing gallery source must stay import-only equivalent to its canonical Manim fixture",
);

const vectorFieldEntry = readyEntries.find(
  (entry) => entry.id === "compatible-arrow-vector-field-static",
);
assert.ok(vectorFieldEntry, "static ArrowVectorField must be a ready compatibility example");
assert.equal(vectorFieldEntry.parity_status, "parity-qualified");
assert.equal(vectorFieldEntry.parity_fixture, "arrow-vector-field-static");
assert.ok(vectorFieldEntry.features.includes("pixel-parity"));
assert.ok(vectorFieldEntry.features.includes("time-parity"));
const vectorFieldSource = await readFile(
  new URL(`./${vectorFieldEntry.path}`, import.meta.url),
  "utf8",
);
const vectorFieldCanonical = await readFile(
  new URL("../parity/manim-v0.21/core-examples/arrow_vector_field.py", import.meta.url),
  "utf8",
);
assert.equal(
  vectorFieldSource.replace("from noon import *", "from manim import *"),
  vectorFieldCanonical,
  "ArrowVectorField gallery source must stay import-only equivalent to its qualified canonical fixture",
);

const tigerEntry = readyEntries.find((entry) => entry.id === "compatible-svg-tiger-morph");
assert.ok(tigerEntry, "Ghostscript Tiger SVG morph must be a ready compatibility example");
assert.equal(tigerEntry.category, "manim-compatible/svg");
assert.match(tigerEntry.upstream, /linebender\/vello\/blob\/1e63b4a40ccb484f82e1d85b83df97ab95bcfbe7\/assets\/Ghostscript_Tiger\.svg$/);
const tigerGalleryEntry = gallery.examples.find((entry) => entry.id === tigerEntry.id);
assert.equal(tigerGalleryEntry.thumbnail, tigerEntry.thumbnail, "absolute HTTPS tiger thumbnail must remain absolute");
const tigerSource = await readFile(new URL(`./${tigerEntry.path}`, import.meta.url), "utf8");
assert.match(tigerSource, /from pathlib import Path as FilePath/, "tiger demo must not let Noon's Path export shadow pathlib.Path");
assert.match(tigerSource, /_DEMO_DIR = FilePath\(gettempdir\(\)\)/, "tiger demo filesystem paths must use the pathlib alias");
assert.equal((tigerSource.match(/SVGMobject\(/g) ?? []).length, 2, "tiger demo must parse the Tiger and unrelated target as distinct SVG families");
assert.equal((tigerSource.match(/tiger\.copy\(\)/g) ?? []).length, 1, "tiger demo must retain one original Tiger family for the return transform");
assert.equal((tigerSource.match(/Transform\(/g) ?? []).length, 2, "tiger demo must morph to the unrelated SVG family and back");
assert.equal((tigerSource.match(/<path\b/g) ?? []).length, 6, "unrelated target must be an independently authored six-path SVG family");
assert.match(tigerSource, /rocket = SVGMobject\(str\(_TARGET_PATH\), height=5\.2\)/, "tiger demo must construct an independent target family");
assert.doesNotMatch(tigerSource, /for index, leaf in enumerate/, "tiger demo must not fake deformation by rearranging copied Tiger leaves");
assert.doesNotMatch(tigerSource, /<(?:rect|circle|ellipse|polygon|polyline)\b/, "unrelated target must stay within qualified plain SVG path topology");
assert.match(tigerSource, /1e63b4a40ccb484f82e1d85b83df97ab95bcfbe7\/assets\/Ghostscript_Tiger\.svg/);
assert.doesNotMatch(tigerSource, /SVGMobject\.from_string/, "demo should exercise ordinary file-backed SVGMobject authoring");

const plottingEntry = readyEntries.find((entry) => entry.id === "noon-coordinate-plotting");
assert.ok(plottingEntry, "coordinate plotting must be a ready compatibility example");
assert.equal(plottingEntry.parity_status, "candidate");
assert.equal(plottingEntry.category, "manim-compatible/plotting");
const plottingSource = await readFile(new URL(`./${plottingEntry.path}`, import.meta.url), "utf8");
assert.match(plottingSource, /Axes\(/, "coordinate plotting gallery example must construct Axes");
assert.match(plottingSource, /\.plot\(/, "coordinate plotting gallery example must plot a function");
assert.match(plottingSource, /\.plot_samples\(/, "coordinate plotting gallery example must plot samples");
assert.match(plottingSource, /Time \(s\)/, "coordinate plotting gallery example must include its label");

const numberPlaneEntry = readyEntries.find((entry) => entry.id === "noon-number-plane");
assert.ok(numberPlaneEntry, "NumberPlane must be a ready compatibility example");
assert.equal(numberPlaneEntry.parity_status, "candidate");
assert.equal(numberPlaneEntry.category, "manim-compatible/plotting");
const numberPlaneSource = await readFile(new URL(`./${numberPlaneEntry.path}`, import.meta.url), "utf8");
assert.match(numberPlaneSource, /NumberPlane\(/, "NumberPlane gallery example must construct NumberPlane");
assert.match(numberPlaneSource, /faded_line_ratio/, "NumberPlane gallery example must show faded subdivisions");
assert.match(numberPlaneSource, /\.c2p\(/, "NumberPlane gallery example must use coordinate conversion");
assert.match(numberPlaneSource, /\.plot\(/, "NumberPlane gallery example must plot a function");
assert.match(numberPlaneSource, /\.scale\(/, "NumberPlane gallery example must exercise grouped transforms");
const implicitEntry = readyEntries.find(entry => entry.id === "noon-implicit-plotting");
assert.ok(implicitEntry);
assert.equal(implicitEntry.parity_status, "candidate");
const implicitSource = await readFile(new URL(`./${implicitEntry.path}`, import.meta.url), "utf8");
assert.match(implicitSource, /plot_implicit_curve\(/);
assert.match(implicitSource, /min_depth=4, max_quads=600/);

const areaEntry = readyEntries.find((entry) => entry.id === "noon-area-helpers");
assert.ok(areaEntry, "area helpers must be a ready compatibility example");
assert.equal(areaEntry.parity_status, "candidate");
assert.equal(areaEntry.parity_fixture, "area-helpers");
assert.equal(areaEntry.category, "manim-compatible/plotting");
const areaSource = await readFile(new URL(`./${areaEntry.path}`, import.meta.url), "utf8");
assert.match(areaSource, /Axes\(/, "area gallery example must construct Axes");
assert.match(areaSource, /\.get_area\(/, "area gallery example must construct a shaded area");
assert.match(areaSource, /\.get_riemann_rectangles\(/, "area gallery example must construct Riemann rectangles");
assert.match(areaSource, /input_sample_type="center"/, "area gallery example must exercise midpoint sampling");

console.log("✓ Noon-authored Manim-compatible gallery examples");
