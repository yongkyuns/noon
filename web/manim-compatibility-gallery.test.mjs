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
assert.equal(gallery.examples.length, 11);
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
  await access(new URL(`./${entry.thumbnail}`, import.meta.url));

  const source = await readFile(new URL(`./${entry.path}`, import.meta.url), "utf8");
  assert.match(source, /from noon import\b/, `${entry.id}: public source must import Noon`);
  for (const pattern of noonOnlyPatterns) {
    assert.doesNotMatch(
      source,
      pattern,
      `${entry.id}: Manim-compatible source must not depend on Noon-only helper ${pattern}`,
    );
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

console.log("✓ Noon-authored Manim-compatible gallery examples");
