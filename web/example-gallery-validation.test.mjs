import assert from "node:assert/strict";

import { normalizeGalleryManifest } from "./example-gallery.js";

function readyEntry(overrides = {}) {
  return {
    id: "valid-example",
    title: "Valid example",
    status: "ready",
    reuse: "source-equivalent-manim-v0.21",
    path: "python/examples/manim_valid_example.py",
    thumbnail: "thumbnails/manim/valid-example.webp",
    features: ["Circle"],
    parity_status: "candidate",
    ...overrides,
  };
}

for (const title of [undefined, null, "", "   "]) {
  assert.throws(
    () => normalizeGalleryManifest({ entries: [readyEntry({ title })] }),
    /requires a non-empty title/,
    `ready gallery title ${String(title)} must fail at the manifest boundary`,
  );
}

for (const order of [Number.NaN, Number.POSITIVE_INFINITY, "not-a-number"]) {
  assert.throws(
    () => normalizeGalleryManifest({ entries: [readyEntry({ order })] }),
    /order must be finite/,
    `ready gallery order ${String(order)} must fail before sorting`,
  );
}

for (const thumbnailTime of [-0.1, Number.NaN, Number.POSITIVE_INFINITY, "not-a-number"]) {
  assert.throws(
    () =>
      normalizeGalleryManifest({
        entries: [readyEntry({ thumbnail_time: thumbnailTime })],
      }),
    /thumbnail_time must be a finite non-negative number/,
    `thumbnail time ${String(thumbnailTime)} must fail before rendering`,
  );
}

const normalized = normalizeGalleryManifest({
  entries: [readyEntry({ order: "2", thumbnail_time: "0.5" })],
}).examples[0];
assert.equal(normalized.order, 2, "finite numeric order strings retain existing normalization semantics");
assert.equal(
  normalized.thumbnailTime,
  0.5,
  "finite numeric thumbnail-time strings retain existing normalization semantics",
);

console.log("✓ runnable gallery scalar metadata validation");
