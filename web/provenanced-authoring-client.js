import {
  AUTHORING_CHANNEL,
  AUTHORING_PROTOCOL_VERSION,
  PythonAuthoringClient,
} from "./authoring-client.js";

const expectedPaths = Object.freeze({
  worker: "./python-worker.js",
  wasm: "./pkg/noon_web_bg.wasm",
  glue: "./pkg/noon_web.js",
  verifier: "./runtime-build-verifier.js",
});

// Thin host wrapper over the canonical PythonAuthoringClient. It observes the
// existing ready message only; authoring requests, semantic execution, and
// lifecycle ownership remain entirely in PythonAuthoringClient.
export class ProvenancedPythonAuthoringClient extends PythonAuthoringClient {
  #buildIdentityPromise;

  constructor(worker = createAuthoringWorker()) {
    let resolveIdentity;
    let rejectIdentity;
    const identityPromise = new Promise((resolve, reject) => {
      resolveIdentity = resolve;
      rejectIdentity = reject;
    });
    identityPromise.catch(() => {});
    super(worker);
    this.#buildIdentityPromise = identityPromise;
    worker.addEventListener("message", (event) => {
      try {
        const message = event.data;
        if (message?.channel !== AUTHORING_CHANNEL ||
            message?.protocolVersion !== AUTHORING_PROTOCOL_VERSION ||
            message?.type !== "ready") {
          return;
        }
        resolveIdentity(validateRuntimeBuildIdentity(message.buildIdentity));
      } catch (error) {
        rejectIdentity(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  async ready() {
    await super.ready();
    return this.#buildIdentityPromise;
  }
}

export function validateRuntimeBuildIdentity(value) {
  if (!isRecord(value) || !hasExactKeys(value, ["schema", "sourceRevision", "files", "buildId"]) ||
      value.schema !== 1 || !/^[0-9a-f]{64}$/.test(value.buildId) ||
      (value.sourceRevision !== null && !/^[0-9a-f]{40}$/.test(value.sourceRevision)) ||
      !isRecord(value.files) || !hasExactKeys(value.files, Object.keys(expectedPaths))) {
    throw new Error("Python authoring worker returned an invalid runtime build identity");
  }
  const files = {};
  for (const key of Object.keys(expectedPaths)) {
    const descriptor = value.files[key];
    if (!isRecord(descriptor) || !hasExactKeys(descriptor, ["path", "sha256"]) ||
        descriptor.path !== expectedPaths[key] || !/^[0-9a-f]{64}$/.test(descriptor.sha256)) {
      throw new Error(`Python authoring worker returned invalid ${key} build provenance`);
    }
    files[key] = Object.freeze({ path: descriptor.path, sha256: descriptor.sha256 });
  }
  Object.freeze(files);
  return Object.freeze({
    schema: 1,
    sourceRevision: value.sourceRevision,
    files,
    buildId: value.buildId,
  });
}

function createAuthoringWorker() {
  return new Worker(new URL("./python-worker.js", import.meta.url), {
    name: "noon-python-authoring",
    type: "module",
  });
}

function hasExactKeys(value, expected) {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
