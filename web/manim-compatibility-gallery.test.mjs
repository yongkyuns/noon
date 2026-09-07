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
assert.equal(gallery.examples.length, 8);
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
  assert.match(source, /from noon import \*/, `${entry.id}: public source must import Noon`);
  for (const pattern of noonOnlyPatterns) {
    assert.doesNotMatch(
      source,
      pattern,
      `${entry.id}: Manim-compatible source must not depend on Noon-only helper ${pattern}`,
    );
  }
}

console.log("✓ Noon-authored Manim-compatible gallery examples");
