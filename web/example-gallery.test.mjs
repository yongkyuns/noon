import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import {
  exampleUrl,
  filterGalleryExamples,
  normalizeGalleryManifest,
  parityLabel,
  requestedExampleId,
} from "./example-gallery.js";

const manifest = JSON.parse(
  await readFile(new URL("./python/examples/manim_tutorial_manifest.json", import.meta.url), "utf8"),
);
const gallery = normalizeGalleryManifest(manifest);
const readyEntries = manifest.entries.filter((entry) => entry.status === "ready");
assert.equal(
  gallery.examples.length,
  readyEntries.length,
  "gallery must expose every ready manifest entry without a hard-coded catalog size",
);
assert.ok(
  gallery.examples.every((entry) => entry.path.startsWith("./python/examples/manim_")),
  "public Manim gallery entries must come from the checked-in Manim example corpus",
);
assert.ok(
  gallery.examples.every(
    (entry) =>
      entry.thumbnail.startsWith("./thumbnails/manim/") &&
      (entry.thumbnail.endsWith(".webp") || entry.thumbnail.endsWith(".svg")),
  ),
  "ready gallery entries must use a checked-in Manim thumbnail/poster",
);
assert.ok(
  readyEntries.every(
    (entry) =>
      typeof entry.upstream_source === "string" &&
      entry.upstream_source.startsWith("parity/manim-v0.21/upstream-examples/"),
  ),
  "every public example must point to its canonical upstream source fixture",
);
assert.equal(
  gallery.examples.find((entry) => entry.id === "parity-draw-border-then-fill-styled-square")
    ?.parityStatus,
  "parity-qualified",
  "exact-source DrawBorderThenFill keeps its qualified raster/timeline evidence",
);
for (const id of [
  "manim-dot-example",
  "manim-ellipse-example",
  "manim-show-uncreate",
  "manim-show-increasing-subsets",
  "manim-add-with-run-time",
  "manim-succession-example",
  "manim-grow-from-point",
  "manim-grow-from-center",
  "manim-grow-from-edge",
  "manim-spin-in-from-nothing",
  "manim-rotating-demo",
  "manim-using-focus-on",
  "manim-lagged-start-map",
]) {
  assert.equal(
    gallery.examples.find((entry) => entry.id === id)?.parityStatus,
    "candidate",
    `${id}: exact upstream source should be public before separate raster qualification`,
  );
}
assert.equal(
  gallery.examples.some((entry) => entry.id === "manim-using-indicate"),
  false,
  "UsingIndicate must stay blocked while retained-family Indicate remains partial",
);
for (const syntheticProbeId of [
  "parity-dot-ellipse",
  "parity-add-wait-lagged-start-map",
  "parity-grow-point-center-edge",
  "parity-uncreate-styled-square",
  "parity-focus-on-point",
  "parity-rotating-centered",
  "parity-show-increasing-subsets-two-shapes",
  "parity-show-submobjects-one-by-one-two-shapes",
  "parity-indicate-square",
]) {
  assert.equal(
    gallery.examples.some((entry) => entry.id === syntheticProbeId),
    false,
    `${syntheticProbeId}: synthetic parity probes must not substitute for upstream gallery examples`,
  );
}

const filtered = filterGalleryExamples(gallery.examples, { query: "DifferentRotations" });
assert.deepEqual(filtered.map((entry) => entry.id), ["parity-different-rotations"]);
assert.equal(
  filterGalleryExamples(gallery.examples, { parityStatus: "parity-qualified" }).length,
  readyEntries.filter((entry) => entry.parity_status === "parity-qualified").length,
);
assert.ok(
  filterGalleryExamples(gallery.examples, { category: "composition" }).some(
    (entry) => entry.id === "manim-add-with-run-time",
  ),
);

const syntheticManifest = {
  entries: Array.from({ length: 100 }, (_, index) => ({
    id: `synthetic-${index}`,
    title: `Synthetic example ${index}`,
    summary: index % 10 === 0 ? "search-target" : "ordinary",
    status: "ready",
    reuse: "source-equivalent-manim-v0.21",
    path: `python/examples/synthetic-${index}.py`,
    thumbnail: `thumbnails/manim/synthetic-${index}.webp`,
    features: [index % 2 === 0 ? "Circle" : "Square", "pixel-parity", "time-parity"],
    category: index % 2 === 0 ? "parity/a" : "parity/b",
    parity_status: index % 5 === 0 ? "parity-qualified" : "candidate",
    parity_fixture: `synthetic-${index}`,
    order: index,
  })),
};
const synthetic = normalizeGalleryManifest(syntheticManifest).examples;
assert.equal(synthetic.length, 100, "gallery schema must remain cheap/simple at 100 entries");
assert.equal(filterGalleryExamples(synthetic, { query: "search-target" }).length, 10);
assert.equal(filterGalleryExamples(synthetic, { category: "parity/a" }).length, 50);
assert.equal(
  filterGalleryExamples(synthetic, { parityStatus: "parity-qualified" }).length,
  20,
);

assert.equal(
  requestedExampleId({ search: "?example=parity-square-to-circle" }),
  "parity-square-to-circle",
);
assert.equal(
  exampleUrl("parity-create-circle", {
    href: "https://example.test/web/index.html?foo=1#demo",
  }),
  "/web/index.html?foo=1&example=parity-create-circle#demo",
);
assert.equal(parityLabel("candidate"), "Parity candidate");
assert.equal(parityLabel("parity-qualified"), "Parity qualified");

assert.throws(
  () =>
    normalizeGalleryManifest({
      entries: [
        {
          id: "bad-adaptation",
          title: "Bad",
          status: "ready",
          reuse: "original-noon-adaptation",
          path: "python/example.py",
          thumbnail: "thumbnail.webp",
          features: ["Circle"],
          parity_status: "candidate",
        },
      ],
    }),
  /source-equivalent ManimCE/,
);

console.log("✓ Manim gallery manifest contract");
