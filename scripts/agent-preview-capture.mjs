import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";

const UNKNOWN_BUILD = Object.freeze({
  engineRevision: null,
  wasmSha256: null,
  workerSha256: null,
  buildId: null,
});
const MAX_SOURCE_BYTES = 1_000_000;

function ownData(record, key) {
  if (record === null || typeof record !== "object") {
    throw new TypeError("preview capture sample must be an object");
  }
  const descriptor = Object.getOwnPropertyDescriptor(record, key);
  if (!descriptor || !Object.hasOwn(descriptor, "value")) {
    throw new TypeError(`preview capture sample requires data field ${key}`);
  }
  return descriptor.value;
}

function validateSource(source) {
  if (typeof source !== "string" || source.trim() === "") {
    throw new TypeError("preview capture source must be non-empty text");
  }
  if (Buffer.byteLength(source, "utf8") > MAX_SOURCE_BYTES) {
    throw new RangeError("preview capture source exceeds byte limit");
  }
  return source;
}

/**
 * Trusted capture-to-artifact adapter for one preview session.
 *
 * The adapter computes submitted-source identity itself and binds a private
 * FrameArtifactStore scope. It stores only already-captured PNG bytes and
 * sample metadata observed from the preview host. Build/scene/frame identities
 * remain explicit null until the runner can genuinely observe them; checkout
 * revisions are never substituted for loaded-build evidence.
 *
 * This is artifact ownership only: it does not launch source, expose execution,
 * add seek/inspection, authenticate a caller, or claim process isolation.
 */
export class PreviewFrameArtifactSession {
  #scope;
  #closed = false;
  #sourceSha256;

  constructor({ store, sessionId, source }) {
    if (store === null || typeof store !== "object" || typeof store.openScope !== "function") {
      throw new TypeError("preview capture requires a FrameArtifactStore-like owner");
    }
    const submitted = validateSource(source);
    this.#sourceSha256 = createHash("sha256").update(submitted, "utf8").digest("hex");
    this.#scope = store.openScope({
      sessionId,
      sourceSha256: this.#sourceSha256,
      build: { ...UNKNOWN_BUILD },
    });
  }

  get sourceSha256() {
    return this.#sourceSha256;
  }

  retain(sample, png) {
    if (this.#closed) throw new Error("preview artifact session is closed");
    const error = ownData(sample, "error");
    const presented = ownData(sample, "presented");
    const requestedTime = ownData(sample, "requestedTime");
    const publishedTime = ownData(sample, "publishedTime");
    const backend = ownData(sample, "rendererBackend");
    if (error !== null) throw new Error("cannot retain a failed preview sample");
    if (presented !== true) throw new Error("cannot retain an unpresented preview sample");
    return this.#scope.putFrame({
      png,
      requestedTime,
      publishedTime,
      backend,
      sceneRevision: null,
      frameRevision: null,
    });
  }

  read(id) {
    if (this.#closed) throw new Error("preview artifact session is closed");
    return this.#scope.getFrame(id);
  }

  close() {
    if (this.#closed) return false;
    this.#closed = true;
    return this.#scope.close();
  }
}
