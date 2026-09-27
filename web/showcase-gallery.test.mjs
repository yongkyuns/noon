import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { isShowcaseRequest, loadShowcaseGallery, normalizeShowcaseManifest, SHOWCASE_MANIFEST } from "./showcase-gallery.js";
import { loadGalleryManifest, parityLabel } from "./example-gallery.js";

const manifest = JSON.parse(await readFile(new URL("./python/examples/noon_showcase_manifest.json", import.meta.url), "utf8"));
const copy = () => structuredClone(manifest);

test("showcase is the default; explicit reference and legacy links retain their catalog", () => {
  assert.equal(isShowcaseRequest(undefined), true);
  assert.equal(isShowcaseRequest({ search: "" }), true);
  assert.equal(isShowcaseRequest({ search: "?example=parity-create-circle" }), false);
  assert.equal(isShowcaseRequest({ search: "?catalog=showcase" }), true);
  assert.equal(isShowcaseRequest({ search: "?catalog=reference" }), false);
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
  assert.equal(parityLabel("noon-showcase"), "Noon showcase");
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

test("playback capability is finite and nonreplayable capabilities require an explanation", () => {
  const unsupported = copy();
  unsupported.entries[0].playback_capability = "best-effort-replay";
  assert.throws(() => normalizeShowcaseManifest(unsupported), /unsupported playback capability/);

  const missingExplanation = copy();
  const reactive = missingExplanation.entries.find((entry) => entry.id === "showcase-reactive-relationships");
  reactive.playback_capability = "nonreplayable-host-callbacks";
  delete reactive.playback_limitation;
  assert.throws(() => normalizeShowcaseManifest(missingExplanation), /requires a user-facing limitation/);

  const gallery = normalizeShowcaseManifest(manifest);
  const normalizedReactive = gallery.examples.find((entry) => entry.id === "showcase-reactive-relationships");
  assert.equal(normalizedReactive.playbackCapability, "nonreplayable-host-callbacks");
  assert.match(normalizedReactive.summary, /cannot be retained for deterministic replay/);
  assert.ok(gallery.examples.filter((entry) => entry.id !== normalizedReactive.id)
    .every((entry) => entry.playbackCapability === "deterministic-retained-replay"));
});

test("the checked-in catalog declares only the reactive-relationships lesson nonreplayable", () => {
  assert.deepEqual(
    manifest.entries.filter((entry) => entry.playback_capability === "nonreplayable-host-callbacks")
      .map((entry) => entry.id),
    ["showcase-reactive-relationships"],
  );
});

test("loader and facade use the showcase catalog without fetching legacy manifests", async () => {
  const requested = [];
  const fakeFetch = async (url) => {
    requested.push(url);
    return { ok: true, json: async () => manifest };
  };
  assert.equal((await loadShowcaseGallery(fakeFetch)).examples.length, manifest.entries.length);
  assert.equal((await loadGalleryManifest(undefined, fakeFetch, { search: "" })).examples.length, manifest.entries.length);
  assert.deepEqual(requested, [SHOWCASE_MANIFEST, SHOWCASE_MANIFEST]);
  await assert.rejects(loadShowcaseGallery(async () => ({ ok: false, status: 503 })), /503/);
});
