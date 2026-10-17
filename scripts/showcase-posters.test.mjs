import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { posterEvidence, assertRetainedPoster } from "./showcase-posters.mjs";

const web = new URL("../web/", import.meta.url);
const read = path => readFile(new URL(path, web));
const manifest = JSON.parse(await read("python/examples/noon_showcase_manifest.json"));
const evidence = JSON.parse(await read("thumbnails/showcase/capture-evidence.json"));
const assertPoster = (...args) => assertRetainedPoster(...args, evidence.runtimeBuildIdentity);
const cudaEvidence = JSON.parse(await read("thumbnails/showcase/cuda-transpose-capture-evidence.json"));

function sourceBoundPosters() {
  const sources = [evidence, cudaEvidence];
  const records = new Map();
  for (const source of sources) {
    assert.equal(source.schema, 1);
    assert.equal(source.publication, "captured-preview");
    assert.equal(source.captureRevision, source.runtimeBuildIdentity.sourceRevision);
    for (const poster of source.posters) {
      assert.equal(records.has(poster.id), false, poster.id + ": duplicated capture evidence");
      records.set(poster.id, { poster, identity: source.runtimeBuildIdentity });
    }
  }
  return records;
}


test("every showcase card has a retained, source-bound real capture", async () => {
  assert.equal(evidence.schema, 1);
  assert.equal(evidence.publication, "captured-preview");
  assert.equal(manifest.publication, "curated");
  assert.equal(evidence.captureRevision, evidence.runtimeBuildIdentity.sourceRevision);
  const captures = sourceBoundPosters();
  assert.deepEqual([...captures.keys()], manifest.entries.map(entry => entry.id));
  for (const entry of manifest.entries) {
    const { poster, identity } = captures.get(entry.id);
    assertRetainedPoster(entry, poster, await read(entry.path), await read(entry.thumbnail), identity);
  }
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

test("the CUDA card reuses source-bound actual Noon browser-fidelity PNG evidence", async () => {
  const entry = manifest.entries.find(item => item.id === "showcase-cuda-transpose");
  const poster = cudaEvidence.posters[0];
  assert.equal(cudaEvidence.posters.length, 1);
  assert.equal(poster.id, entry.id);
  assert.equal(cudaEvidence.backend, "webgpu");
  assert.equal(cudaEvidence.origin?.workflowRun, 39055762669);
  assert.equal(cudaEvidence.origin?.artifact, 12110234543);
  assert.equal(cudaEvidence.origin?.frame, "column-3-speed-1-none/215-bank-end.png");
  assert.equal(cudaEvidence.origin?.sourceMode, "deterministic-cpu-reference");
  assert.equal(poster.sample.completionProbe, false);
  assert.equal(poster.sample.publishedTime, entry.thumbnail_time);
  const source = await read(entry.path), png = await read(entry.thumbnail);
  assertRetainedPoster(entry, poster, source, png, cudaEvidence.runtimeBuildIdentity);
  assert.throws(() => assertRetainedPoster(entry, poster, source, png, evidence.runtimeBuildIdentity),
    /different runtime build/);
  const otherSource = Buffer.concat([source, Buffer.from("# altered")]);
  assert.throws(() => assertRetainedPoster(entry, poster, otherSource, png, cudaEvidence.runtimeBuildIdentity),
    /recapture/);
});
