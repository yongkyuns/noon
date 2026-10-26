import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { runtimeBuildIdentityCore } from "./build-runtime-identity.mjs";
import { mergePosterCatalog, validatePosterCatalog } from "./showcase-poster-catalog.mjs";

function build(letter) {
  const core = runtimeBuildIdentityCore({
    sourceRevision: letter.repeat(40),
    hashes: Object.fromEntries(["worker", "wasm", "glue", "verifier"].map(key => [key, letter.repeat(64)])),
  });
  return { ...core, buildId: createHash("sha256").update(JSON.stringify(core)).digest("hex") };
}

const originalBuild = build("a"), refreshedBuild = build("b");
const manifest = { publication: "curated", entries: ["first", "second", "third"].map(name => ({ id: `showcase-${name}` })) };
const record = (id, identity = originalBuild) => ({
  id, captureBuildId: identity.buildId, backend: "webgl",
  sourceSha256: "c".repeat(64), thumbnailTime: 1,
  image: { pngSha256: "d".repeat(64), width: 960, height: 540 },
  sample: { requestedTime: 1, publishedTime: 1 },
});
const catalog = () => ({
  schema: 2, publication: "captured-preview-catalog", builds: { [originalBuild.buildId]: originalBuild },
  posters: manifest.entries.map(entry => record(entry.id)),
});
const batch = (ids = ["showcase-second"]) => ({
  schema: 1, publication: "captured-preview", captureRevision: refreshedBuild.sourceRevision,
  runtimeBuildIdentity: refreshedBuild, backend: "webgpu",
  posters: ids.map(id => ({ ...record(id, refreshedBuild), sourceSha256: "e".repeat(64) })),
});

test("partial refresh preserves untouched records and their original runtime identity", async () => {
  const before = catalog();
  const snapshot = structuredClone(before);
  const merged = await mergePosterCatalog(manifest, before, batch());
  assert.deepEqual(before, snapshot, "merge must not mutate the retained catalog");
  assert.deepEqual(merged.posters.map(poster => poster.id), manifest.entries.map(entry => entry.id));
  assert.deepEqual(merged.posters[0], before.posters[0]);
  assert.deepEqual(merged.posters[2], before.posters[2]);
  assert.equal(merged.posters[1].captureBuildId, refreshedBuild.buildId);
  assert.equal(merged.posters[1].sourceSha256, "e".repeat(64));
  assert.equal(merged.posters[1].backend, "webgpu");
  assert.deepEqual(merged.builds[originalBuild.buildId], originalBuild);
  assert.deepEqual(merged.builds[refreshedBuild.buildId], refreshedBuild);
});

test("captures merge in manifest order and retire identities after their last poster is refreshed", async () => {
  const first = await mergePosterCatalog(manifest, catalog(), batch(["showcase-third", "showcase-first"]));
  const second = await mergePosterCatalog(manifest, first, batch());
  assert.deepEqual(second.posters.map(poster => poster.id), manifest.entries.map(entry => entry.id));
  assert.ok(second.posters.every(poster => poster.captureBuildId === refreshedBuild.buildId));
  assert.deepEqual(Object.keys(second.builds), [refreshedBuild.buildId]);
});

test("missing, relabeled, dirty or corrupted build identities reject the catalog", async () => {
  for (const mutate of [
    value => { delete value.builds[originalBuild.buildId]; },
    value => { value.builds[originalBuild.buildId].buildId = "0".repeat(64); },
    value => { value.builds[originalBuild.buildId].sourceRevision = null; },
    value => { value.builds[originalBuild.buildId].files.wasm.sha256 = "0".repeat(64); },
    value => { value.posters[0].captureBuildId = "0".repeat(64); },
    value => { value.posters.push(value.posters[0]); },
    value => { delete value.posters[0].sourceSha256; },
    value => { delete value.posters[0].image.pngSha256; },
  ]) {
    const invalid = structuredClone(catalog());
    mutate(invalid);
    await assert.rejects(validatePosterCatalog(invalid));
  }
});

test("unknown, duplicated or mixed-build batch records cannot replace retained posters", async () => {
  for (const update of [
    batch(["showcase-unknown"]), batch(["showcase-second", "showcase-second"]),
    { ...batch(), captureRevision: originalBuild.sourceRevision },
    { ...batch(), posters: [record("showcase-second")] },
    { ...batch(), backend: "fake" }, { ...batch(), posters: [] },
  ]) {
    const before = catalog();
    await assert.rejects(mergePosterCatalog(manifest, before, update));
    assert.deepEqual(before, catalog());
  }
});
