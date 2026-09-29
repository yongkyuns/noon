import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { posterEvidence, assertRetainedPoster } from "./showcase-posters.mjs";

const web = new URL("../web/", import.meta.url);
const read = path => readFile(new URL(path, web));
const manifest = JSON.parse(await read("python/examples/noon_showcase_manifest.json"));
const evidence = JSON.parse(await read("thumbnails/showcase/capture-evidence.json"));

test("every showcase card has a retained, source-bound real capture", async () => {
  assert.equal(evidence.schema, 1);
  assert.equal(evidence.publication, "captured-preview");
  assert.equal(manifest.publication, "curated");
  assert.equal(evidence.captureRevision, evidence.runtimeBuildIdentity.sourceRevision);
  assert.deepEqual(evidence.posters.map(poster => poster.id), manifest.entries.map(entry => entry.id));
  for (const [index, entry] of manifest.entries.entries()) {
    assertRetainedPoster(entry, evidence.posters[index], await read(entry.path), await read(entry.thumbnail));
  }
});

test("source, image, timing and framing changes cannot silently reuse a poster", async () => {
  const entry = manifest.entries[0], record = evidence.posters[0];
  const source = await read(entry.path), png = await read(entry.thumbnail);
  assert.throws(() => assertRetainedPoster(entry, record, Buffer.concat([source, Buffer.from("# changed")]), png), /recapture/);
  const changed = Buffer.from(png); changed[changed.length - 1] ^= 1;
  assert.throws(() => assertRetainedPoster(entry, record, source, changed), /retained image/);
  assert.throws(() => assertRetainedPoster({ ...entry, thumbnail_time: 1 }, record, source, png), /time changed/);
  assert.throws(() => assertRetainedPoster(entry, { ...record, image: { ...record.image, width: 100 } }, source, png));
  assert.throws(() => assertRetainedPoster(entry, { ...record, sample: { ...record.sample, publishedTime: 0 } }, source, png));
});

test("native-input posters require drag, background no-op, reset, and current source evidence", async () => {
  const entry = manifest.entries.find(item => item.playback_capability === "nonreplayable-native-input");
  assert.ok(entry, "native-input lesson is in the showcase manifest");
  const source = await read(entry.path);
  // The native-input capture is awaiting its first retained poster; use an existing
  // decoded showcase PNG only to exercise this metadata contract.
  const png = await read(manifest.entries[0].thumbnail);
  const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");
  const posterHash = sha256(png);
  const record = {
    id: entry.id,
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
  assertRetainedPoster(entry, record, source, png);
  for (const field of ["automaticRestore", "pointerDrag", "backgroundNoOp", "runRestoresBase"]) {
    const interaction = { ...record.interaction };
    delete interaction[field];
    assert.throws(() => assertRetainedPoster(entry, { ...record, interaction }, source, png),
      new RegExp(field));
  }
  assert.throws(() => assertRetainedPoster(entry, {
    ...record,
    interaction: { ...record.interaction, changedPixelsOutsideRightSideRoi: 1 },
  }, source, png), /ROI confinement/);
  assert.throws(() => assertRetainedPoster(entry, {
    ...record,
    interaction: { ...record.interaction, selectedImage: { pngSha256: "0".repeat(64) } },
  }, source, png));
  assert.throws(() => assertRetainedPoster(entry, record,
    Buffer.concat([source, Buffer.from("\\n# stale-source check")]), png), /source changed/);
});

test("failed or mixed-build reports cannot generate a retained evidence record", () => {
  for (const report of [
    {}, { checkoutRevision: "a".repeat(40), servedBuildIdentity: { sourceRevision: "b".repeat(40) } },
    { checkoutRevision: "a".repeat(40), servedBuildIdentity: { sourceRevision: "a".repeat(40), buildId: "b".repeat(64) }, backendRequested: "fake", results: [] },
  ]) assert.throws(() => posterEvidence(manifest, report));
});
