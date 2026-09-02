import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import { loadGalleryManifest, normalizeGalleryManifest } from "./example-gallery.js";

const stressManifest = JSON.parse(
  await readFile(new URL("./python/examples/manim_stress_manifest.json", import.meta.url), "utf8"),
);
const stressGallery = normalizeGalleryManifest(stressManifest);

assert.equal(stressManifest.reference.version, "0.21.0");
assert.equal(stressGallery.examples.length, 1);
const stress = stressGallery.examples[0];
assert.equal(stress.id, "manim-parity-stress-grid");
assert.equal(stress.reuse, "manim-compatible-parity-v0.21");
assert.equal(stress.parityStatus, "candidate");
assert.equal(stress.parityFixture, "mixed-object-parity-stress");
assert.equal(stress.category, "stress");
assert.ok(stress.features.includes("Text"));
assert.ok(stress.features.includes("Transform"));
assert.ok(stress.features.includes("FadeIn"));
assert.ok(stress.features.includes("FadeOut"));
assert.ok(stress.features.includes("pixel-parity"));
assert.ok(stress.features.includes("time-parity"));

const canonicalSource = await readFile(
  new URL("../parity/manim-v0.21/stress-examples/mixed_object_parity_stress.py", import.meta.url),
  "utf8",
);
const noonSource = await readFile(
  new URL("./python/examples/manim_parity_stress_grid.py", import.meta.url),
  "utf8",
);
assert.equal(
  canonicalSource.match(/from manim import \*/g)?.length,
  1,
  "canonical stress scene must have exactly one Manim star import",
);
assert.equal(
  noonSource,
  canonicalSource.replace("from manim import *", "from noon import *"),
  "Noon stress demo must differ from canonical Manim source by the import only",
);

const primaryManifest = {
  reference: { version: "0.21.0" },
  entries: [
    {
      id: "base-example",
      title: "Base example",
      status: "ready",
      reuse: "source-equivalent-manim-v0.21",
      path: "python/examples/manim_base.py",
      thumbnail: "thumbnails/manim/base.svg",
      features: ["Circle"],
      parity_status: "candidate",
      order: 10,
    },
  ],
};
const requests = [];
const merged = await loadGalleryManifest(undefined, async (url) => {
  requests.push(url);
  const manifest = url.endsWith("manim_stress_manifest.json") ? stressManifest : primaryManifest;
  return {
    ok: true,
    status: 200,
    async json() {
      return manifest;
    },
  };
});
assert.deepEqual(requests, [
  "./python/examples/manim_tutorial_manifest.json",
  "./python/examples/manim_stress_manifest.json",
]);
assert.deepEqual(
  merged.examples.map((example) => example.id),
  ["base-example", "manim-parity-stress-grid"],
  "default gallery load must merge and order custom stress workloads with the Manim corpus",
);

console.log("✓ custom Manim parity stress gallery contract");
