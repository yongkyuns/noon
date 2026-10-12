import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { isShowcaseRequest, loadShowcaseGallery, normalizeShowcaseManifest, SHOWCASE_MANIFEST } from "./showcase-gallery.js";
import { loadGalleryManifest, parityLabel } from "./example-gallery.js";
import { installShowcasePresentation } from "./showcase-presentation.js";

const manifest = JSON.parse(await readFile(new URL("./python/examples/noon_showcase_manifest.json", import.meta.url), "utf8"));
const copy = () => structuredClone(manifest);

test("catalog navigation preserves the served page path", () => {
  for (const showcase of [false, true]) {
    let link;
    const document = {
      documentElement: { dataset: {} }, head: { append() {} },
      getElementById: () => null,
      createElement: () => ({}),
      querySelector: selector => selector === ".topbar" ? { append(value) { link = value; } } : null,
    };
    installShowcasePresentation(document, showcase);
    for (const pathname of ["/", "/web/", "/web/index.html"]) {
      const destination = new URL(link.href, `https://noon.test${pathname}?example=old`);
      assert.equal(destination.pathname, pathname);
      assert.equal(destination.search, showcase ? "?catalog=reference" : "?catalog=showcase");
    }
  }
});

test("live renderer metrics, including labeled FPS, appear in both catalogs", () => {
  for (const showcase of [false, true]) {
    const outputs = new Map(["metric-fps", "metric-frame-gap", "metric-objects", "metric-draws", "metric-upload", "metric-time"]
      .map((id) => [id, { id, textContent: "—", replaceWith(label) { this.replacedBy = label; } }]));
    const metrics = {
      hidden: true,
      classList: { values: new Set(), add(name) { this.values.add(name); } },
      setAttribute(name, value) { this[name] = value; },
    };
    const link = {};
    const document = {
      documentElement: { dataset: {} }, head: { append() {} },
      getElementById: (id) => outputs.get(id) ?? null,
      createElement: () => ({ append(output) { this.output = output; } }),
      querySelector: (selector) => selector === ".topbar" ? { append(value) { link.value = value; } }
        : selector === ".metrics" ? metrics : null,
    };
    installShowcasePresentation(document, showcase);
    const fps = outputs.get("metric-fps").replacedBy;
    assert.equal(fps.textContent, "FPS · target 60");
    assert.match(fps.title, /presentations per second/);
    assert.match(fps.title, /Static holds can show 0/);
    const frameGap = outputs.get("metric-frame-gap").replacedBy;
    assert.equal(frameGap.textContent, "Frame gap · p95 / max");
    assert.match(frameGap.title, /Renderer submission intervals during continuous animation/);
    assert.match(frameGap.title, /16\.7 ms/);
    assert.match(frameGap.title, /not physical display scanout/);
    assert.equal(metrics.hidden, false);
    assert.equal(metrics["aria-hidden"], "false");
    assert.ok(metrics.classList.values.has("catalog-live-metrics"));
    for (const id of ["metric-objects", "metric-draws", "metric-upload", "metric-time"]) {
      assert.ok(outputs.get(id).replacedBy, `${id} remains visible`);
    }
  }
});

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
  const pointer = gallery.examples.find((entry) => entry.id === "showcase-pointer-selection");
  const pointerSource = manifest.entries.find((entry) => entry.id === "showcase-pointer-selection");
  assert.ok(pointer.features.includes("on_click"));
  assert.ok(pointer.features.includes("Indicate"));
  assert.equal(pointer.interaction, null);
  assert.equal(pointerSource.interaction, undefined);
  assert.equal(pointer.inspectionZoom, true);
  assert.match(pointerSource.host_setup, /wheel\/trackpad zoom/);
  assert.ok(gallery.examples.filter(entry => entry.id !== pointer.id).every(entry => !entry.inspectionZoom));
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
    (value) => { value.entries[0].inspection_zoom = "true"; },
    (value) => { value.entries[0].inspection_zoom = true; value.entries[0].host_setup = ""; },
  ]) {
    const value = copy();
    mutate(value);
    assert.throws(() => normalizeShowcaseManifest(value));
  }

  const legacyInteraction = copy();
  legacyInteraction.entries[0].interaction = { type: "pointer-fill-selection" };
  legacyInteraction.entries[0].host_setup = "";
  assert.throws(() => normalizeShowcaseManifest(legacyInteraction), /interactive scenes must disclose/);
});

test("playback capability is finite and all nonreplayable capabilities require an explanation", () => {
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
  const redraw = gallery.examples.find((entry) => entry.id === "showcase-always-redraw");
  assert.equal(redraw.playbackCapability, "nonreplayable-host-callbacks");
  assert.match(redraw.summary, /always_redraw callbacks/);
  const camera = gallery.examples.find((entry) => entry.id === "showcase-camera-follows-path");
  assert.equal(camera.playbackCapability, "nonreplayable-host-callbacks");
  assert.match(camera.playbackLimitation, /Run/);
  const nativeDrag = gallery.examples.find((entry) => entry.id === "showcase-translation-drag");
  assert.equal(nativeDrag.playbackCapability, "nonreplayable-native-input");
  assert.equal(nativeDrag.playbackLimitation, "Dragging changes this scene. Use Run to reset; seeking and restart are unavailable.");
  assert.ok(gallery.examples.filter((entry) => ![normalizedReactive.id, redraw.id, camera.id, nativeDrag.id].includes(entry.id))
    .every((entry) => entry.playbackCapability === "deterministic-retained-replay"));
});

test("the checked-in catalog declares callback and native-input lessons nonreplayable", () => {
  assert.deepEqual(
    manifest.entries.filter((entry) => entry.playback_capability?.startsWith("nonreplayable-"))
      .map((entry) => [entry.id, entry.playback_capability]),
    [["showcase-reactive-relationships", "nonreplayable-host-callbacks"],
      ["showcase-camera-follows-path", "nonreplayable-host-callbacks"],
      ["showcase-always-redraw", "nonreplayable-host-callbacks"],
      ["showcase-translation-drag", "nonreplayable-native-input"]],
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
