import { Buffer } from "node:buffer";
import { createHash, randomUUID } from "node:crypto";
import { performance } from "node:perf_hooks";
import { types } from "node:util";

const DEFAULT_LIMITS = Object.freeze({
  maxScopes: 16, maxArtifacts: 128, maxFramesPerScope: 32,
  maxArtifactBytes: 8 * 1024 * 1024, maxTotalBytes: 64 * 1024 * 1024,
  maxDimension: 8192, maxPixels: 16_777_216, ttlMs: 300_000,
});
const BUILD_KEYS = ["engineRevision", "wasmSha256", "workerSha256", "buildId"];
const FRAME_KEYS = ["png", "requestedTime", "publishedTime", "backend", "sceneRevision", "frameRevision"];
const PNG_SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
// Buffer instances can shadow .buffer/.length/.byteLength with data or getters.
// Read native slots for backing-store rejection, quota checks and allocation.
const typedArrayPrototype = Object.getPrototypeOf(Uint8Array.prototype);
const typedArrayBuffer = Object.getOwnPropertyDescriptor(typedArrayPrototype, "buffer").get;
const typedArrayByteLength = Object.getOwnPropertyDescriptor(typedArrayPrototype, "byteLength").get;

export class ArtifactError extends Error {
  constructor(code, message) { super(message); this.name = "ArtifactError"; this.code = code; }
}
const fail = (code, message) => { throw new ArtifactError(code, message); };
const check = (condition, code, message) => { if (!condition) fail(code, message); };

// Copy only specified data fields. Accessors, unknown fields and omissions are
// rejected instead of running user callbacks or retaining arbitrary metadata.
function record(value, keys, name) {
  check(value !== null && typeof value === "object" &&
    [Object.prototype, null].includes(Object.getPrototypeOf(value)), "INVALID_INPUT", `${name} must be a plain record`);
  const descriptors = Object.getOwnPropertyDescriptors(value);
  check(Reflect.ownKeys(descriptors).length === keys.length &&
    keys.every((key) => Object.hasOwn(descriptors, key) && Object.hasOwn(descriptors[key], "value")),
  "INVALID_INPUT", `${name} must contain exactly: ${keys.join(", ")}`);
  return Object.fromEntries(keys.map((key) => [key, descriptors[key].value]));
}
function text(value, name, maxBytes = 256) {
  check(typeof value === "string" && value.trim().length > 0 && !value.includes("\0") &&
    Buffer.byteLength(value, "utf8") <= maxBytes, "INVALID_INPUT", `${name} must be bounded non-empty text`);
  return value;
}
function hash(value, name, length = 64) {
  check(typeof value === "string" && new RegExp(`^[a-f0-9]{${length}}$`).test(value),
    "INVALID_INPUT", `${name} must be a lowercase hexadecimal identity`);
  return value;
}
function time(value, name) {
  check(Number.isFinite(value) && value >= 0, "INVALID_INPUT", `${name} must be finite and nonnegative`);
  return value;
}
function revision(value, name) {
  check(value === null || (Number.isSafeInteger(value) && value >= 0),
    "INVALID_INPUT", `${name} must be a nonnegative safe integer or explicit null`);
  return value;
}
function buildIdentity(value) {
  const build = record(value, BUILD_KEYS, "build");
  for (const key of BUILD_KEYS) {
    if (build[key] === null) continue; // Unknown is explicit, never inferred from this checkout.
    if (key === "buildId") text(build[key], key);
    else hash(build[key], key, key === "engineRevision" ? 40 : 64);
  }
  return Object.freeze(build);
}

// Framing/dimension check for PNGs from the trusted capture adapter. This is NOT
// a CRC/DEFLATE decoder or sanitizer for untrusted uploads. No pixel allocation.
// PNG specification: https://www.w3.org/TR/png-3/#11IHDR
function pngDimensions(png, limits) {
  const valid = (condition) => check(condition, "INVALID_PNG", "invalid PNG framing or dimensions");
  valid(png.length >= 57 && png.subarray(0, 8).equals(PNG_SIGNATURE));
  valid(png.readUInt32BE(8) === 13 && png.toString("ascii", 12, 16) === "IHDR");
  const width = png.readUInt32BE(16), height = png.readUInt32BE(20);
  valid(width > 0 && height > 0 && width <= limits.maxDimension && height <= limits.maxDimension);
  valid(width <= Math.floor(limits.maxPixels / height));
  let offset = 8, sawData = false, dataEnded = false;
  while (offset < png.length) {
    valid(png.length - offset >= 12);
    const length = png.readUInt32BE(offset);
    valid(length <= 2_147_483_647 && length <= png.length - offset - 12);
    const kind = png.toString("latin1", offset + 4, offset + 8);
    valid(/^[A-Za-z]{4}$/.test(kind));
    if (kind === "IHDR") valid(offset === 8);
    if (kind === "IDAT") { valid(!dataEnded); sawData = true; }
    else if (sawData) dataEnded = true;
    if (kind === "IEND") {
      valid(length === 0 && sawData && offset + 12 === png.length);
      return { width, height };
    }
    offset += length + 12;
  }
  fail("INVALID_PNG", "PNG lacks final IEND");
}

function copyBytes(bytes) {
  // Buffer.from(smallBuffer) can use a shared pool. Returning its .buffer could
  // expose another stored frame's allocation. Use independent ArrayBuffers.
  const copy = Buffer.alloc(typedArrayByteLength.call(bytes));
  copy.set(bytes);
  return copy;
}

/**
 * Optional host artifact ownership, NOT runner session/scene state or execution.
 * A trusted adapter supplies already-captured PNGs and observed provenance.
 * This store validates the metadata shape, not the truth of source/build claims.
 * No process, file, network, renderer, Python, MCP or R1 supervisor dependency.
 *
 * openScope binds a caller-provided session/source/build to an in-process
 * capability. Do not reconstruct that capability from an untrusted session ID.
 * CLI and MCP adapters must retain it under their own authenticated ownership.
 *
 * Fixed monotonic TTL begins at putFrame, never renewed by reads. getFrame
 * expires its target lazily; sweepExpired explicitly reclaims all expired bytes.
 * Call sweepExpired before admission when reclamation is desired; no hidden
 * full-store scan, eviction, timer, or background scheduler runs on put/get.
 * close/dispose revoke access and release store references (not secure erasure).
 *
 * Bounds apply to store-owned encoded bytes and counts, not decoded pixels,
 * returned caller copies, JS allocator overhead, wire/base64 size or process RSS.
 * put/get are O(frame bytes); close is O(scope frames); sweep is O(all frames).
 */
export class FrameArtifactStore {
  #limits;
  #clock;
  #lastTime = 0;
  #scopes = new Map();
  #bytes = 0;
  #count = 0;
  #disposed = false;

  constructor({ limits = {}, clock = () => performance.now() } = {}) {
    check(limits !== null && typeof limits === "object" && !Array.isArray(limits), "INVALID_INPUT", "limits must be a record");
    check(Reflect.ownKeys(limits).every((key) => Object.hasOwn(DEFAULT_LIMITS, key)), "INVALID_INPUT", "unknown artifact limit");
    this.#limits = Object.freeze({ ...DEFAULT_LIMITS, ...limits });
    for (const [key, value] of Object.entries(this.#limits)) {
      check(Number.isSafeInteger(value) && value > 0 && value <= 2_147_483_647,
        "INVALID_INPUT", `${key} must be a positive bounded integer`);
    }
    check(typeof clock === "function", "INVALID_INPUT", "clock must be a function");
    this.#clock = clock; // Trusted monotonic clock injection for deterministic tests.
  }

  #now() {
    const now = this.#clock();
    check(Number.isFinite(now) && now >= this.#lastTime && now <= Number.MAX_SAFE_INTEGER - this.#limits.ttlMs,
      "INVALID_CLOCK", "artifact clock must be finite, monotonic and bounded");
    this.#lastTime = now;
    return now;
  }
  #active(scope) {
    check(!this.#disposed, "STORE_CLOSED", "artifact store is disposed");
    if (scope) check(this.#scopes.get(scope.sessionId) === scope, "SCOPE_CLOSED", "artifact scope is closed");
  }
  #remove(scope, id) {
    const entry = scope.frames.get(id);
    if (!entry) return false;
    scope.frames.delete(id);
    this.#count--;
    this.#bytes -= entry.png.length;
    return true;
  }
  #close(scope) {
    if (this.#scopes.get(scope.sessionId) !== scope) return false;
    for (const id of scope.frames.keys()) this.#remove(scope, id);
    this.#scopes.delete(scope.sessionId);
    return true;
  }

  openScope(input) {
    this.#active();
    const { sessionId, sourceSha256, build } = record(input, ["sessionId", "sourceSha256", "build"], "scope");
    const provenance = Object.freeze({ sessionId: text(sessionId, "sessionId"),
      sourceSha256: hash(sourceSha256, "sourceSha256"), build: buildIdentity(build) });
    check(!this.#scopes.has(sessionId), "SCOPE_EXISTS", "session already has an artifact scope");
    check(this.#scopes.size < this.#limits.maxScopes, "SCOPE_LIMIT", "artifact scope limit reached");
    const scope = { ...provenance, frames: new Map() };
    this.#scopes.set(sessionId, scope);
    return Object.freeze({
      putFrame: (frame) => this.#put(scope, provenance, frame),
      getFrame: (id) => this.#get(scope, id),
      deleteFrame: (id) => { this.#active(scope); return this.#remove(scope, id); },
      close: () => this.#close(scope),
    });
  }

  #put(scope, provenance, input) {
    this.#active(scope);
    const frame = record(input, FRAME_KEYS, "frame");
    const metadata = Object.freeze({ ...provenance,
      requestedTime: time(frame.requestedTime, "requestedTime"),
      publishedTime: time(frame.publishedTime, "publishedTime"),
      backend: text(frame.backend, "backend", 64),
      sceneRevision: revision(frame.sceneRevision, "sceneRevision"),
      frameRevision: revision(frame.frameRevision, "frameRevision"),
    });
    const png = frame.png;
    check(types.isUint8Array(png) && Buffer.isBuffer(png) &&
      !types.isSharedArrayBuffer(typedArrayBuffer.call(png)), "INVALID_INPUT", "png must be an unshared Buffer");
    const now = this.#now();
    this.#active(scope);
    const byteLength = typedArrayByteLength.call(png);
    check(byteLength > 0 && byteLength <= this.#limits.maxArtifactBytes, "PAYLOAD_LIMIT", "frame exceeds encoded byte limit");
    check(scope.frames.size < this.#limits.maxFramesPerScope && this.#count < this.#limits.maxArtifacts,
      "FRAME_LIMIT", "retained frame count limit reached");
    check(byteLength <= this.#limits.maxTotalBytes - this.#bytes, "STORAGE_LIMIT", "retained byte limit reached");
    const owned = copyBytes(png);
    const dimensions = pngDimensions(owned, this.#limits);
    const id = randomUUID();
    check(!scope.frames.has(id), "ID_COLLISION", "artifact identity collision");
    const descriptor = Object.freeze({ id, mimeType: "image/png", ...dimensions,
      byteLength: owned.length, sha256: createHash("sha256").update(owned).digest("hex"),
      provenance: metadata, retentionMs: this.#limits.ttlMs,
    });
    scope.frames.set(id, { descriptor, png: owned, expiresAt: now + this.#limits.ttlMs });
    this.#bytes += owned.length;
    this.#count++;
    return descriptor;
  }

  #get(scope, id) {
    this.#active(scope);
    const now = this.#now();
    this.#active(scope);
    const entry = scope.frames.get(id);
    // Unknown, expired and cross-scope IDs intentionally have the same response.
    if (entry && now >= entry.expiresAt) this.#remove(scope, id);
    check(entry && now < entry.expiresAt, "ARTIFACT_NOT_FOUND", "artifact is unavailable in this scope");
    return Object.freeze({ descriptor: entry.descriptor, png: copyBytes(entry.png) });
  }

  sweepExpired() {
    this.#active();
    const now = this.#now();
    this.#active();
    let removed = 0;
    for (const scope of this.#scopes.values()) {
      for (const [id, entry] of scope.frames) {
        if (now >= entry.expiresAt) { this.#remove(scope, id); removed++; }
      }
    }
    return removed;
  }
  stats() {
    return Object.freeze({ scopes: this.#scopes.size, artifacts: this.#count,
      encodedBytes: this.#bytes, disposed: this.#disposed });
  }
  dispose() {
    if (this.#disposed) return false;
    for (const scope of this.#scopes.values()) this.#close(scope);
    this.#disposed = true;
    return true;
  }
}
