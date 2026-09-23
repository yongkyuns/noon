import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { isShowcaseRequest, loadShowcaseGallery, normalizeShowcaseManifest, SHOWCASE_MANIFEST } from "./showcase-gallery.js";
import { loadGalleryManifest, parityLabel } from "./example-gallery.js";

const manifest = JSON.parse(await readFile(new URL("./python/examples/noon_showcase_manifest.json", import.meta.url), "utf8"));
const copy = () => structuredClone(manifest);

test("showcase routes are explicit; legacy and default routes remain reference", () => {
  assert.equal(isShowcaseRequest(undefined), false);
  assert.equal(isShowcaseRequest({ search: "" }), false);
  assert.equal(isShowcaseRequest({ search: "?example=parity-create-circle" }), false);
  assert.equal(isShowcaseRequest({ search: "?catalog=showcase" }), true);
  assert.equal(isShowcaseRequest({ search: "?example=showcase-first-scene" }), true);
  assert.equal(isShowcaseRequest({ search: "?catalog=reference&example=showcase-first-scene" }), false);
});

test("lessons have unique sources, outcomes, and real-poster destinations", async () => {
  const gallery = normalizeShowcaseManifest(manifest);
  assert.equal(gallery.examples.length, manifest.entries.length);
  assert.equal(gallery.reference, null);
  assert.ok(gallery.examples.every((entry) => entry.parityStatus === "noon-showcase"));
  assert.ok(gallery.examples.findIndex((entry) => entry.performance) < 3, "dynamic composition stays featured");
  for (const entry of gallery.examples) {
    const source = await readFile(new URL(entry.path, import.meta.url), "utf8");
    assert.ok(source.includes("from noon import *"));
    assert.equal(/^\s*assert\b/m.test(source), false, `${entry.id}: no embedded regression assertions`);
  }
  const pointer = gallery.examples.find((entry) => entry.interaction);
  assert.equal(pointer.interaction.type, "pointer-fill-selection");
  assert.match(pointer.summary, /copying the Python scene alone/i);
  assert.equal(parityLabel("noon-showcase"), "Noon showcase preview");
  assert.equal(parityLabel("parity-qualified"), "Parity qualified");
});

test("invalid or misleading publication metadata is rejected", () => {
  for (const mutate of [
    (value) => { value.publication = "approved"; },
    (value) => { value.entries[1].id = value.entries[0].id; },
    (value) => { value.entries[1].path = value.entries[0].path; },
    (value) => { value.entries[1].primary_feature = value.entries[0].primary_feature; },
    (value) => { value.entries[0].thumbnail = "https://example.test/invented.svg"; },
    (value) => { value.entries[0].thumbnail_time = 1e9; },
    (value) => { value.entries[0].beats = []; },
    (value) => { value.entries.find((entry) => entry.interaction).host_setup = ""; },
  ]) {
    const value = copy();
    mutate(value);
    assert.throws(() => normalizeShowcaseManifest(value));
  }
});

test("loader and facade use the showcase catalog without fetching legacy manifests", async () => {
  const requested = [];
  const fakeFetch = async (url) => {
    requested.push(url);
    return { ok: true, json: async () => manifest };
  };
  assert.equal((await loadShowcaseGallery(fakeFetch)).examples.length, manifest.entries.length);
  assert.equal((await loadGalleryManifest(undefined, fakeFetch, { search: "?catalog=showcase" })).examples.length, manifest.entries.length);
  assert.deepEqual(requested, [SHOWCASE_MANIFEST, SHOWCASE_MANIFEST]);
  await assert.rejects(loadShowcaseGallery(async () => ({ ok: false, status: 503 })), /503/);
});
