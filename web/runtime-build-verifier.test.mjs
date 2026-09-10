import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import { validateRuntimeBuildIdentity } from "./runtime-build-verifier.js";

function validIdentity() {
  const core = {
    schema: 1,
    sourceRevision: "a".repeat(40),
    files: {
      worker: { path: "./python-worker.js", sha256: "b".repeat(64) },
      wasm: { path: "./pkg/noon_web_bg.wasm", sha256: "c".repeat(64) },
      glue: { path: "./pkg/noon_web.js", sha256: "d".repeat(64) },
      verifier: { path: "./runtime-build-verifier.js", sha256: "e".repeat(64) },
    },
  };
  return {
    ...core,
    buildId: createHash("sha256").update(JSON.stringify(core)).digest("hex"),
  };
}

test("runtime verifier accepts an exact content identity and freezes observations", async () => {
  const identity = validIdentity();
  const observed = await validateRuntimeBuildIdentity(identity);
  assert.deepEqual(observed, identity);
  assert.equal(Object.isFrozen(observed), true);
  assert.equal(Object.isFrozen(observed.files), true);
  assert.equal(Object.isFrozen(observed.files.verifier), true);
});

test("runtime verifier rejects envelope, path, hash, and build digest drift", async () => {
  const valid = validIdentity();
  for (const invalid of [
    { ...valid, schema: 2 },
    { ...valid, sourceRevision: "short" },
    { ...valid, buildId: "0".repeat(64) },
    { ...valid, extra: true },
    { ...valid, files: { ...valid.files, wasm: { ...valid.files.wasm, path: "./other.wasm" } } },
    { ...valid, files: { ...valid.files, worker: { ...valid.files.worker, sha256: "bad" } } },
    { ...valid, files: { worker: valid.files.worker, wasm: valid.files.wasm, glue: valid.files.glue } },
  ]) {
    await assert.rejects(validateRuntimeBuildIdentity(invalid), /runtime build identity|provenance|digest/);
  }
});
