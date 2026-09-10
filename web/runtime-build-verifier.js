const EXPECTED_RUNTIME_PATHS = Object.freeze({
  worker: "./python-worker.js",
  wasm: "./pkg/noon_web_bg.wasm",
  glue: "./pkg/noon_web.js",
  verifier: "./runtime-build-verifier.js",
});

export async function loadRuntimeBuild() {
  const response = await fetch(new URL("./runtime-build-identity.json", import.meta.url), { redirect: "error" });
  if (!response.ok) {
    throw new Error(`Unable to load Noon runtime build identity: HTTP ${response.status}`);
  }
  const identity = await validateRuntimeBuildIdentity(await response.json());
  const [workerBytes, wasmBytes, glueBytes, verifierBytes] = await Promise.all([
    fetchVerifiedRuntimeBytes(identity.files.worker),
    fetchVerifiedRuntimeBytes(identity.files.wasm),
    fetchVerifiedRuntimeBytes(identity.files.glue),
    fetchVerifiedRuntimeBytes(identity.files.verifier),
  ]);
  // The worker/glue/verifier fetches prove the served module graph matches the
  // recorded build under the controlled runner. The verified WASM bytes are
  // additionally passed directly to wasm-bindgen by python-worker.js.
  void workerBytes;
  void glueBytes;
  void verifierBytes;
  return Object.freeze({ identity, wasmBytes });
}

export async function validateRuntimeBuildIdentity(payload) {
  if (!isRecord(payload) || !hasExactKeys(payload, ["schema", "sourceRevision", "files", "buildId"]) ||
      payload.schema !== 1 || !/^[0-9a-f]{64}$/.test(payload.buildId) ||
      (payload.sourceRevision !== null && !/^[0-9a-f]{40}$/.test(payload.sourceRevision)) ||
      !isRecord(payload.files) || !hasExactKeys(payload.files, Object.keys(EXPECTED_RUNTIME_PATHS))) {
    throw new Error("Noon runtime build identity has an invalid envelope");
  }
  const files = {};
  for (const key of Object.keys(EXPECTED_RUNTIME_PATHS)) {
    const descriptor = payload.files[key];
    if (!isRecord(descriptor) || !hasExactKeys(descriptor, ["path", "sha256"]) ||
        descriptor.path !== EXPECTED_RUNTIME_PATHS[key] || !/^[0-9a-f]{64}$/.test(descriptor.sha256)) {
      throw new Error(`Noon runtime build identity has invalid ${key} provenance`);
    }
    files[key] = Object.freeze({ path: descriptor.path, sha256: descriptor.sha256 });
  }
  const core = {
    schema: 1,
    sourceRevision: payload.sourceRevision,
    files: {
      worker: files.worker,
      wasm: files.wasm,
      glue: files.glue,
      verifier: files.verifier,
    },
  };
  const expectedBuildId = await sha256Hex(new TextEncoder().encode(JSON.stringify(core)));
  if (expectedBuildId !== payload.buildId) {
    throw new Error("Noon runtime build identity digest does not match its contents");
  }
  Object.freeze(core.files);
  return Object.freeze({ ...core, buildId: payload.buildId });
}

async function fetchVerifiedRuntimeBytes(descriptor) {
  const response = await fetch(new URL(descriptor.path, import.meta.url), { redirect: "error" });
  if (!response.ok) {
    throw new Error(`Unable to load Noon runtime file ${descriptor.path}: HTTP ${response.status}`);
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  const actual = await sha256Hex(bytes);
  if (actual !== descriptor.sha256) {
    throw new Error(`Noon runtime file hash does not match build identity: ${descriptor.path}`);
  }
  return bytes;
}

async function sha256Hex(bytes) {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return Array.from(digest, (value) => value.toString(16).padStart(2, "0")).join("");
}

function hasExactKeys(value, expected) {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
