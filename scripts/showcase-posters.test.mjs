import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { posterEvidence, assertRetainedPoster, assertRetainedCudaFidelityPoster } from "./showcase-posters.mjs";

const web = new URL("../web/", import.meta.url);
const read = path => readFile(new URL(path, web));
const manifest = JSON.parse(await read("python/examples/noon_showcase_manifest.json"));
const evidence = JSON.parse(await read("thumbnails/showcase/capture-evidence.json"));
const assertPoster = (...args) => assertRetainedPoster(...args, evidence.runtimeBuildIdentity);

test("every showcase card has a retained, source-bound real capture", async () => {
  assert.equal(evidence.schema, 1);
  assert.equal(evidence.publication, "captured-preview");
  assert.equal(manifest.publication, "curated");
  assert.equal(evidence.captureRevision, evidence.runtimeBuildIdentity.sourceRevision);
  const existingEntries = manifest.entries.filter(entry =>
    !["showcase-cuda-transpose", "showcase-background-music"].includes(entry.id));
  assert.deepEqual(evidence.posters.slice(0, existingEntries.length).map(poster => poster.id),
    existingEntries.map(entry => entry.id), "all previously captured source-bound showcase posters must remain");
  assert.ok(evidence.posters.length === existingEntries.length ||
    evidence.posters.length === existingEntries.length + 1,
    "capture evidence must be the original batch or a fully refreshed batch");
  for (const [index, entry] of existingEntries.entries()) {
    assertPoster(entry, evidence.posters[index], await read(entry.path), await read(entry.thumbnail));
  }
  if (evidence.posters.length > existingEntries.length) {
    const cuda = manifest.entries.find(entry => entry.id === "showcase-cuda-transpose");
    assertPoster(cuda, evidence.posters[existingEntries.length],
      await read(cuda.path), await read(cuda.thumbnail));
  }
});

test("the audio lesson has a source-bound real capture from its qualified browser build", async () => {
  const entry = manifest.entries.find(entry => entry.id === "showcase-background-music");
  assert.ok(entry);
  const capture = JSON.parse(await read("thumbnails/showcase/showcase-background-music-evidence.json"));
  assert.equal(capture.schema, 1);
  assert.equal(capture.publication, "captured-preview");
  assert.match(capture.captureRevision, /^[a-f0-9]{40}$/);
  assert.equal(capture.captureRevision, capture.runtimeBuildIdentity.sourceRevision);
  assert.equal(capture.qualification.status, "passed");
  assert.equal(capture.qualification.sourceDirty, false);
  assert.equal(capture.qualification.script, "scripts/python-background-audio-smoke.mjs");
  assert.deepEqual(capture.posters.map(record => record.id), [entry.id]);
  assertRetainedPoster(entry, capture.posters[0], await read(entry.path), await read(entry.thumbnail),
    capture.runtimeBuildIdentity);
});

test("source, image, timing and framing changes cannot silently reuse a poster", async () => {
  const entry = manifest.entries[0], record = evidence.posters[0];
  const source = await read(entry.path), png = await read(entry.thumbnail);
  assert.throws(() => assertPoster(entry, record, Buffer.concat([source, Buffer.from("# changed")]), png), /recapture/);
  const changed = Buffer.from(png); changed[changed.length - 1] ^= 1;
  assert.throws(() => assertPoster(entry, record, source, changed), /retained image/);
  assert.throws(() => assertPoster({ ...entry, thumbnail_time: 1 }, record, source, png), /time changed/);
  assert.throws(() => assertPoster(entry, { ...record, image: { ...record.image, width: 100 } }, source, png));
  assert.throws(() => assertPoster(entry, { ...record, sample: { ...record.sample, publishedTime: 0 } }, source, png));
});

test("a poster copied from another build cannot inherit the global capture identity", async () => {
  const entry = manifest.entries[0], record = evidence.posters[0];
  const source = await read(entry.path), png = await read(entry.thumbnail);
  assert.throws(() => assertPoster(entry, { ...record, captureBuildId: "0".repeat(64) }, source, png),
    /different runtime build/);
  const missingIdentity = { ...record }; delete missingIdentity.captureBuildId;
  assert.throws(() => assertPoster(entry, missingIdentity, source, png));
});

test("native-input posters require drag, background no-op, reset, and current source evidence", async () => {
  const entry = manifest.entries.find(item => item.playback_capability === "nonreplayable-native-input");
  assert.ok(entry, "native-input lesson is in the showcase manifest");
  const source = await read(entry.path);
  // Use an independent showcase PNG to isolate the metadata admission contract.
  const png = await read(manifest.entries[0].thumbnail);
  const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");
  const posterHash = sha256(png);
  const record = {
    id: entry.id,
    captureBuildId: evidence.runtimeBuildIdentity.buildId,
    sourceSha256: sha256(source),
    thumbnailTime: entry.thumbnail_time,
    image: { pngSha256: posterHash, width: png.readUInt32BE(16), height: png.readUInt32BE(20) },
    sample: { requestedTime: entry.thumbnail_time, publishedTime: entry.thumbnail_time },
    interaction: {
      recipe: "click, drag, then Run",
      automaticRestore: true,
      backgroundNoOp: true,
      pointerDrag: true,
      changedPixelsOutsideRightSideRoi: 0,
      runRestoresBase: true,
      selectedImage: { pngSha256: posterHash },
    },
  };
  assertPoster(entry, record, source, png);
  for (const field of ["automaticRestore", "pointerDrag", "backgroundNoOp", "runRestoresBase"]) {
    const interaction = { ...record.interaction };
    delete interaction[field];
    assert.throws(() => assertPoster(entry, { ...record, interaction }, source, png),
      new RegExp(field));
  }
  assert.throws(() => assertPoster(entry, {
    ...record,
    interaction: { ...record.interaction, changedPixelsOutsideRightSideRoi: 1 },
  }, source, png), /ROI confinement/);
  assert.throws(() => assertPoster(entry, {
    ...record,
    interaction: { ...record.interaction, selectedImage: { pngSha256: "0".repeat(64) } },
  }, source, png));
  assert.throws(() => assertPoster(entry, record,
    Buffer.concat([source, Buffer.from("\\n# stale-source check")]), png), /source changed/);
});

test("failed or mixed-build reports cannot generate a retained evidence record", () => {
  for (const report of [
    {}, { checkoutRevision: "a".repeat(40), servedBuildIdentity: { sourceRevision: "b".repeat(40) } },
    { checkoutRevision: "a".repeat(40), servedBuildIdentity: { sourceRevision: "a".repeat(40), buildId: "b".repeat(64) }, backendRequested: "fake", results: [] },
  ]) assert.throws(() => posterEvidence(manifest, report));
});

test("CUDA showcase has exact renderer-derived, source-bound supplementary poster evidence", async () => {
  const entry = manifest.entries.find(entry => entry.id === "showcase-cuda-transpose");
  assert.ok(entry);
  const record = JSON.parse(await read("thumbnails/showcase/showcase-cuda-transpose-evidence.json"));
  const source = await read(entry.path), png = await read(entry.thumbnail);
  assertRetainedCudaFidelityPoster(entry, record, source, png);
  assert.throws(() => assertRetainedCudaFidelityPoster(
    { ...entry, duration: entry.duration + 1 }, record, source, png), /storyboard duration/);
  for (const mutate of [
    (copy) => { copy.capture.rendererBackend = "CPU"; },
    (copy) => { copy.capture.cudaValidated = true; },
    (copy) => { copy.capture.artifactId = 0; },
    (copy) => { copy.capture.authoredDurationSeconds = 81.71; },
    (copy) => { copy.capture.runtimeBuildIdentity.buildId = "wrong"; },
    (copy) => { copy.sample.publishedTime += 1; },
    (copy) => { copy.image.pngSha256 = "0".repeat(64); },
    (copy) => { copy.sourceSha256 = "0".repeat(64); },
  ]) {
    const bad = structuredClone(record);
    mutate(bad);
    assert.throws(() => assertRetainedCudaFidelityPoster(entry, bad, source, png));
  }
  assert.throws(() => assertRetainedCudaFidelityPoster(entry, record,
    Buffer.concat([source, Buffer.from("\\n# changed")]), png), /recapture/);
  const wrongPixels = Buffer.from(png);
  wrongPixels[wrongPixels.length - 1] ^= 1;
  assert.throws(() => assertRetainedCudaFidelityPoster(entry, record, source, wrongPixels),
    /differ from the captured/);
});
