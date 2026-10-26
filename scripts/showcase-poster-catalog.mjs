// Current source-bound posters can come from several independently verified
// captures. Each record keeps its own build; a partial refresh cannot relabel
// untouched images as products of the newest runtime.
import assert from "node:assert/strict";
import { validateRuntimeBuildIdentity } from "../web/runtime-build-verifier.js";

const SHA256 = /^[a-f0-9]{64}$/;
const REVISION = /^[a-f0-9]{40}$/;

function assertPosterRecords(posters, builds) {
  assert.ok(Array.isArray(posters) && posters.length > 0, "poster catalog is empty");
  const ids = new Set();
  for (const record of posters) {
    assert.match(record.id, /^showcase-[a-z0-9-]+$/);
    assert.ok(!ids.has(record.id), `duplicate poster: ${record.id}`);
    ids.add(record.id);
    assert.match(record.captureBuildId, SHA256, `${record.id}: missing capture build ID`);
    assert.ok(Object.hasOwn(builds, record.captureBuildId), `${record.id}: missing capture build identity`);
    assert.ok(["webgl", "webgpu"].includes(record.backend), `${record.id}: invalid capture backend`);
    assert.match(record.sourceSha256, SHA256, `${record.id}: missing source hash`);
    assert.match(record.image?.pngSha256, SHA256, `${record.id}: missing PNG hash`);
  }
}

export async function validatePosterCatalog(catalog) {
  assert.equal(catalog.schema, 2);
  assert.equal(catalog.publication, "captured-preview-catalog");
  assert.ok(catalog.builds && typeof catalog.builds === "object" && !Array.isArray(catalog.builds),
    "poster catalog requires a build identity registry");
  for (const [buildId, identity] of Object.entries(catalog.builds)) {
    assert.equal(buildId, identity?.buildId, "capture registry key differs from its build identity");
    assert.match(identity.sourceRevision, REVISION, "capture runtime requires a clean source revision");
    await validateRuntimeBuildIdentity(identity);
  }
  assertPosterRecords(catalog.posters, catalog.builds);
  return catalog;
}

export async function mergePosterCatalog(manifest, catalog, batch) {
  await validatePosterCatalog(catalog);
  assert.equal(manifest.publication, "curated");
  assert.equal(batch.schema, 1);
  assert.equal(batch.publication, "captured-preview");
  const identity = await validateRuntimeBuildIdentity(batch.runtimeBuildIdentity);
  assert.match(identity.sourceRevision, REVISION, "capture runtime requires a clean source revision");
  assert.equal(batch.captureRevision, identity.sourceRevision, "capture revision differs from its runtime");
  assert.ok(["webgl", "webgpu"].includes(batch.backend), "invalid capture backend");
  const updates = batch.posters.map(record => ({ ...record, backend: batch.backend }));
  assertPosterRecords(updates, { [identity.buildId]: identity });
  const known = new Set(manifest.entries.map(entry => entry.id));
  assert.equal(known.size, manifest.entries.length, "showcase inventory contains duplicate IDs");
  for (const record of [...catalog.posters, ...updates]) {
    assert.ok(known.has(record.id), `unknown showcase poster: ${record.id}`);
  }
  if (Object.hasOwn(catalog.builds, identity.buildId)) {
    assert.deepEqual(catalog.builds[identity.buildId], identity, "conflicting capture build identity");
  }
  const records = new Map(catalog.posters.map(record => [record.id, record]));
  for (const record of updates) records.set(record.id, record);
  const posters = manifest.entries.flatMap(entry => records.has(entry.id) ? [records.get(entry.id)] : []);
  const referencedBuilds = new Set(posters.map(record => record.captureBuildId));
  const merged = {
    schema: 2, publication: "captured-preview-catalog",
    builds: Object.fromEntries(Object.entries({ ...catalog.builds, [identity.buildId]: identity })
      .filter(([buildId]) => referencedBuilds.has(buildId))),
    posters,
  };
  return validatePosterCatalog(merged);
}
