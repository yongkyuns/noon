import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  loadRuntimeBuild,
  validateRuntimeBuildIdentity,
} from "./runtime-build-verifier.js";

const digest = (value) => createHash("sha256").update(value).digest("hex");

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
    buildId: digest(JSON.stringify(core)),
  };
}

function identityForBytes(bytes) {
  const core = {
    schema: 1,
    sourceRevision: null,
    files: {
      worker: { path: "./python-worker.js", sha256: digest(bytes.worker) },
      wasm: { path: "./pkg/noon_web_bg.wasm", sha256: digest(bytes.wasm) },
      glue: { path: "./pkg/noon_web.js", sha256: digest(bytes.glue) },
      verifier: { path: "./runtime-build-verifier.js", sha256: digest(bytes.verifier) },
    },
  };
  return { ...core, buildId: digest(JSON.stringify(core)) };
}

async function withFetch(t, responses) {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    const key = url.pathname.endsWith("/runtime-build-identity.json") ? "identity"
      : url.pathname.endsWith("/python-worker.js") ? "worker"
      : url.pathname.endsWith("/pkg/noon_web_bg.wasm") ? "wasm"
      : url.pathname.endsWith("/pkg/noon_web.js") ? "glue"
      : url.pathname.endsWith("/runtime-build-verifier.js") ? "verifier"
      : null;
    if (key === null || !(key in responses)) return new Response("missing", { status: 404 });
    const value = key === "identity" ? JSON.stringify(responses[key]) : responses[key];
    return new Response(value, { status: 200 });
  };
  t.after(() => { globalThis.fetch = originalFetch; });
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

test("runtime loader returns the exact verified WASM bytes", async (t) => {
  const bytes = {
    worker: "generated worker bytes",
    wasm: Uint8Array.from([0, 97, 115, 109, 1, 2, 3]),
    glue: "generated wasm-bindgen glue",
    verifier: "runtime verifier bytes",
  };
  const identity = identityForBytes(bytes);
  await withFetch(t, { identity, ...bytes });
  const loaded = await loadRuntimeBuild();
  assert.deepEqual(loaded.identity, identity);
  assert.deepEqual([...loaded.wasmBytes], [...bytes.wasm]);
  assert.equal(Object.isFrozen(loaded.identity), true);
});

test("runtime loader fails closed when any served runtime byte differs", async (t) => {
  const bytes = {
    worker: "generated worker bytes",
    wasm: Uint8Array.from([0, 97, 115, 109, 1]),
    glue: "generated wasm-bindgen glue",
    verifier: "runtime verifier bytes",
  };
  const identity = identityForBytes(bytes);
  await withFetch(t, { identity, ...bytes, wasm: Uint8Array.from([0, 97, 115, 109, 9]) });
  await assert.rejects(loadRuntimeBuild(), /hash does not match build identity/);
});
