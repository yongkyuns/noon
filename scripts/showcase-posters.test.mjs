import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { posterEvidence, assertRetainedPoster } from "./showcase-posters.mjs";

const web = new URL("../web/", import.meta.url);
const read = path => readFile(new URL(path, web));
const manifest = JSON.parse(await read("python/examples/noon_showcase_manifest.json"));
const evidence = JSON.parse(await read("thumbnails/showcase/capture-evidence.json"));

test("every preview card has a retained, source-bound real capture", async () => {
  assert.equal(evidence.schema, 1);
  assert.equal(evidence.publication, "captured-preview");
  assert.equal(manifest.publication, "preview", "posters do not authorize default promotion");
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

test("failed or mixed-build reports cannot generate a retained evidence record", () => {
  for (const report of [
    {}, { checkoutRevision: "a".repeat(40), servedBuildIdentity: { sourceRevision: "b".repeat(40) } },
    { checkoutRevision: "a".repeat(40), servedBuildIdentity: { sourceRevision: "a".repeat(40), buildId: "b".repeat(64) }, backendRequested: "fake", results: [] },
  ]) assert.throws(() => posterEvidence(manifest, report));
});
