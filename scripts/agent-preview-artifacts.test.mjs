import assert from "node:assert/strict";
import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { deflateSync } from "node:zlib";
import { ArtifactError, FrameArtifactStore } from "./agent-preview-artifacts.mjs";

// Genuine tiny PNGs, generated test data, NOT images captured from Noon.
function chunk(kind, bytes = Buffer.alloc(0)) {
  const body = Buffer.concat([Buffer.from(kind), bytes]);
  let crc = 0xffffffff;
  for (const byte of body) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  const header = Buffer.alloc(4), trailer = Buffer.alloc(4);
  header.writeUInt32BE(bytes.length);
  trailer.writeUInt32BE((crc ^ 0xffffffff) >>> 0);
  return Buffer.concat([header, body, trailer]);
}
function image(width = 2, height = 1) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width); header.writeUInt32BE(height, 4);
  header[8] = 8; header[9] = 6;
  const pixels = Buffer.alloc((width * 4 + 1) * height, 0);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", header), chunk("IDAT", deflateSync(pixels)), chunk("IEND")]);
}
const PNG = image();
const build = () => ({ engineRevision: "a".repeat(40), wasmSha256: "b".repeat(64),
  workerSha256: null, buildId: "fixture-build" });
const source = (sessionId = "fixture-session") => ({ sessionId, sourceSha256: "c".repeat(64), build: build() });
const frame = (extra = {}) => ({ png: PNG, requestedTime: 1.25, publishedTime: 1.3,
  backend: "webgl2", sceneRevision: null, frameRevision: 0, ...extra });
const code = (expected) => (error) => error instanceof ArtifactError && error.code === expected;
const empty = { scopes: 0, artifacts: 0, encodedBytes: 0, disposed: false };
function setup(limits = {}) {
  let now = 0;
  const store = new FrameArtifactStore({ limits, clock: () => now });
  return { store, scope: store.openScope(source()), setTime: (value) => { now = value; } };
}

test("retains real PNG bytes with opaque ID and exact source/build/time metadata", () => {
  const { store, scope } = setup();
  const descriptor = scope.putFrame(frame());
  const result = scope.getFrame(descriptor.id);
  assert.match(descriptor.id, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  assert.deepEqual(result.png, PNG);
  assert.equal(descriptor.mimeType, "image/png");
  assert.equal(descriptor.width, 2); assert.equal(descriptor.height, 1);
  assert.equal(descriptor.byteLength, PNG.length);
  assert.equal(descriptor.sha256, createHash("sha256").update(PNG).digest("hex"));
  assert.deepEqual(descriptor.provenance, { ...source(), requestedTime: 1.25,
    publishedTime: 1.3, backend: "webgl2", sceneRevision: null, frameRevision: 0 });
  assert.deepEqual(store.stats(), { ...empty, scopes: 1, artifacts: 1, encodedBytes: PNG.length });
});

test("input/output buffers including whole backing ArrayBuffers cannot mutate evidence", () => {
  const { scope } = setup();
  const input = Buffer.from(PNG);
  const descriptor = scope.putFrame(frame({ png: input }));
  input.fill(0);
  const read = scope.getFrame(descriptor.id);
  new Uint8Array(read.png.buffer).fill(0);
  assert.deepEqual(scope.getFrame(descriptor.id).png, PNG);
  assert.equal(scope.getFrame(descriptor.id).descriptor.sha256, descriptor.sha256);
  const second = scope.putFrame(frame());
  new Uint8Array(scope.getFrame(second.id).png.buffer).fill(255);
  assert.deepEqual(scope.getFrame(descriptor.id).png, PNG);
  assert.deepEqual(scope.getFrame(second.id).png, PNG);
});

test("metadata is copied and deeply frozen rather than aliased", () => {
  const { store } = setup();
  const input = source("another-session");
  const scope = store.openScope(input);
  input.sessionId = "changed"; input.build.buildId = "changed";
  const descriptor = scope.putFrame(frame());
  assert.equal(descriptor.provenance.sessionId, "another-session");
  assert.equal(descriptor.provenance.build.buildId, "fixture-build");
  for (const value of [scope, descriptor, descriptor.provenance, descriptor.provenance.build]) assert.ok(Object.isFrozen(value));
  assert.throws(() => { descriptor.provenance.build.buildId = "mutated"; }, TypeError);
});

test("explicit unavailable build and scene/frame identities are preserved as null", () => {
  const { store } = setup();
  const input = source("unknown-build");
  for (const key of Object.keys(input.build)) input.build[key] = null;
  const descriptor = store.openScope(input).putFrame(frame({ frameRevision: null }));
  assert.deepEqual(descriptor.provenance.build, input.build);
  assert.equal(descriptor.provenance.sceneRevision, null);
  assert.equal(descriptor.provenance.frameRevision, null);
});

test("identical frames have distinct IDs, and IDs never grant cross-scope access", () => {
  const { store, scope } = setup();
  const other = store.openScope(source("other"));
  const a = scope.putFrame(frame()), b = scope.putFrame(frame());
  assert.notEqual(a.id, b.id); assert.equal(a.sha256, b.sha256);
  for (const id of [a.id, "../outside.png", "unknown", undefined]) {
    assert.throws(() => other.getFrame(id), code("ARTIFACT_NOT_FOUND"));
    assert.equal(other.deleteFrame(id), false);
  }
  assert.deepEqual(scope.getFrame(a.id).png, PNG);
});

test("closing one scope releases only its frames and revokes stale closures", () => {
  const { store, scope } = setup();
  const other = store.openScope(source("other"));
  const a = scope.putFrame(frame()), b = other.putFrame(frame());
  assert.equal(scope.close(), true); assert.equal(scope.close(), false);
  assert.throws(() => scope.getFrame(a.id), code("SCOPE_CLOSED"));
  assert.throws(() => scope.putFrame(frame()), code("SCOPE_CLOSED"));
  assert.throws(() => scope.deleteFrame(a.id), code("SCOPE_CLOSED"));
  assert.deepEqual(other.getFrame(b.id).png, PNG);
  const reopened = store.openScope(source());
  assert.throws(() => reopened.getFrame(a.id), code("ARTIFACT_NOT_FOUND"));
  scope.close();
  assert.ok(reopened.putFrame(frame()));
  assert.deepEqual(store.stats(), { ...empty, scopes: 2, artifacts: 2, encodedBytes: PNG.length * 2 });
});

test("duplicate active sessions and scope quota are rejected without mutation", () => {
  const { store, scope } = setup({ maxScopes: 1 });
  assert.throws(() => store.openScope(source()), code("SCOPE_EXISTS"));
  assert.throws(() => store.openScope(source("other")), code("SCOPE_LIMIT"));
  assert.deepEqual(store.stats(), { ...empty, scopes: 1 });
  scope.close(); assert.ok(store.openScope(source("other")));
});

for (const [limits, expected] of [
  [{ maxArtifactBytes: PNG.length - 1 }, "PAYLOAD_LIMIT"],
  [{ maxTotalBytes: PNG.length - 1 }, "STORAGE_LIMIT"],
]) {
  test(`${expected} is atomic and checks the actual encoded size`, () => {
    const { store, scope } = setup(limits);
    assert.throws(() => scope.putFrame(frame()), code(expected));
    assert.deepEqual(store.stats(), { ...empty, scopes: 1 });
  });
}

test("exact byte quota is allowed and deletion reclaims capacity", () => {
  const { store, scope } = setup({ maxArtifactBytes: PNG.length, maxTotalBytes: PNG.length });
  const a = scope.putFrame(frame());
  assert.throws(() => scope.putFrame(frame()), code("STORAGE_LIMIT"));
  assert.equal(scope.deleteFrame(a.id), true); assert.equal(scope.deleteFrame(a.id), false);
  assert.throws(() => scope.getFrame(a.id), code("ARTIFACT_NOT_FOUND"));
  assert.ok(scope.putFrame(frame()));
  assert.equal(store.stats().encodedBytes, PNG.length);
});

test("per-scope and global frame quotas are both enforced", () => {
  const { store, scope } = setup({ maxFramesPerScope: 1, maxArtifacts: 2 });
  const other = store.openScope(source("other"));
  const a = scope.putFrame(frame());
  assert.throws(() => scope.putFrame(frame()), code("FRAME_LIMIT"));
  other.putFrame(frame());
  const third = store.openScope(source("third"));
  assert.throws(() => third.putFrame(frame()), code("FRAME_LIMIT"));
  scope.deleteFrame(a.id);
  assert.ok(third.putFrame(frame()));
});

test("TTL expires at the boundary and reads never renew retention", () => {
  const { scope, store, setTime } = setup({ ttlMs: 10 });
  const a = scope.putFrame(frame());
  setTime(9); assert.ok(scope.getFrame(a.id));
  setTime(10); assert.throws(() => scope.getFrame(a.id), code("ARTIFACT_NOT_FOUND"));
  assert.equal(store.stats().encodedBytes, 0); assert.equal(store.stats().artifacts, 0);
});

test("explicit sweep reclaims expired entries without evicting live evidence", () => {
  const { scope, store, setTime } = setup({ ttlMs: 10, maxArtifacts: 2 });
  const a = scope.putFrame(frame());
  setTime(5); const b = scope.putFrame(frame());
  setTime(10);
  assert.throws(() => scope.putFrame(frame()), code("FRAME_LIMIT"));
  assert.equal(store.sweepExpired(), 1); assert.equal(store.sweepExpired(), 0);
  assert.throws(() => scope.getFrame(a.id), code("ARTIFACT_NOT_FOUND"));
  assert.ok(scope.getFrame(b.id)); assert.ok(scope.putFrame(frame()));
});

test("dispose is idempotent and releases every scope, frame and byte", () => {
  const { scope, store } = setup();
  const a = scope.putFrame(frame());
  store.openScope(source("other")).putFrame(frame());
  assert.equal(store.dispose(), true); assert.equal(store.dispose(), false);
  assert.deepEqual(store.stats(), { ...empty, disposed: true });
  for (const operation of [() => scope.getFrame(a.id), () => scope.putFrame(frame()),
    () => store.openScope(source()), () => store.sweepExpired()]) assert.throws(operation, code("STORE_CLOSED"));
  assert.equal(scope.close(), false);
});

for (const [name, value] of [["NaN", NaN], ["infinity", Infinity], ["backward", 4]]) {
  test(`invalid ${name} clock fails without losing stored evidence`, () => {
    const { scope, store, setTime } = setup();
    setTime(5); const a = scope.putFrame(frame());
    const before = store.stats(); setTime(value);
    assert.throws(() => scope.getFrame(a.id), code("INVALID_CLOCK"));
    assert.deepEqual(store.stats(), before);
    setTime(6); assert.ok(scope.getFrame(a.id));
  });
}

test("invalid limits and clock are rejected at construction", () => {
  for (const limits of [{ typo: 1 }, { ttlMs: 0 }, { maxArtifacts: -1 },
    { maxScopes: 1.5 }, { maxPixels: Infinity }, { maxTotalBytes: 2 ** 32 }, null, []]) {
    assert.throws(() => new FrameArtifactStore({ limits }), code("INVALID_INPUT"));
  }
  assert.throws(() => new FrameArtifactStore({ clock: 1 }), code("INVALID_INPUT"));
});

test("source/build metadata omissions, oversized UTF-8 and accessors fail atomically", () => {
  const { store } = setup();
  const before = store.stats();
  const cases = [null, {}, { ...source("new"), extra: 1 }, source("é".repeat(129)),
    { ...source("new"), sourceSha256: "c".repeat(63) }, { ...source("new"), build: {} },
    { ...source("new"), build: { ...build(), workerSha256: undefined } }];
  let called = false;
  const accessor = source("accessor");
  Object.defineProperty(accessor, "sourceSha256", { get() { called = true; return "c".repeat(64); } });
  cases.push(accessor);
  for (const input of cases) assert.throws(() => store.openScope(input), code("INVALID_INPUT"));
  assert.equal(called, false); assert.deepEqual(store.stats(), before);
});

test("invalid frame metadata and shared buffers cannot become retained evidence", () => {
  const { scope, store } = setup();
  const before = store.stats();
  for (const input of [null, {}, frame({ extra: 1 }), frame({ png: "png" }),
    frame({ png: Buffer.from(new SharedArrayBuffer(100)) }), frame({ backend: "" }),
    frame({ requestedTime: -1 }), frame({ publishedTime: NaN }),
    frame({ sceneRevision: undefined }), frame({ frameRevision: 0.5 })]) {
    assert.throws(() => scope.putFrame(input), code("INVALID_INPUT"));
    assert.deepEqual(store.stats(), before);
  }
});

test("PNG signature, chunks, dimensions and trailing data are checked atomically", () => {
  const { scope, store } = setup({ maxDimension: 2, maxPixels: 2 });
  const badSignature = Buffer.from(PNG); badSignature[0] = 0;
  const badLength = Buffer.from(PNG); badLength.writeUInt32BE(0x7fffffff, 33);
  const zeroWidth = Buffer.from(PNG); zeroWidth.writeUInt32BE(0, 16);
  const noData = Buffer.concat([PNG.subarray(0, 33), chunk("IEND")]);
  const duplicateHeader = Buffer.concat([PNG.subarray(0, 33), PNG.subarray(8)]);
  for (const png of [badSignature, badLength, zeroWidth, noData, duplicateHeader,
    PNG.subarray(0, PNG.length - 1), Buffer.concat([PNG, Buffer.from([0])]), image(3, 1), image(2, 2)]) {
    assert.throws(() => scope.putFrame(frame({ png })), code("INVALID_PNG"));
    assert.deepEqual(store.stats(), { ...empty, scopes: 1 });
  }
  assert.ok(scope.putFrame(frame()));
});

test("the store preserves observations; it does not invent scheduler semantics", () => {
  const { scope } = setup();
  const a = scope.putFrame(frame({ requestedTime: 7, publishedTime: 6.9, backend: "observed-backend" }));
  const b = scope.putFrame(frame({ requestedTime: 0, publishedTime: 0, sceneRevision: 5 }));
  assert.equal(a.provenance.publishedTime, 6.9);
  assert.equal(a.provenance.backend, "observed-backend");
  assert.equal(b.provenance.requestedTime, 0);
});

test("repeated independent scopes recover all configured capacity", () => {
  const store = new FrameArtifactStore({ limits: { maxScopes: 1, maxArtifacts: 1, maxTotalBytes: PNG.length } });
  for (let index = 0; index < 100; index++) {
    const scope = store.openScope(source());
    const a = scope.putFrame(frame());
    assert.deepEqual(scope.getFrame(a.id).png, PNG);
    scope.close(); assert.deepEqual(store.stats(), empty);
  }
  store.dispose();
});

// Review regression: instance properties are not evidence of a Buffer's backing
// store or encoded size. No worker/race timing is needed to test admission.
for (const shadow of ["data", "accessor"]) {
  test(`shared backing is rejected with a shadowed buffer ${shadow} property, including other realms`, () => {
    for (const shared of [new SharedArrayBuffer(PNG.length + 8),
      runInNewContext(`new SharedArrayBuffer(${PNG.length + 8})`)]) {
      const { store, scope } = setup();
      const retained = scope.putFrame(frame());
      const before = store.stats();
      const png = Buffer.from(shared, 4, PNG.length);
      png.set(PNG);
      let reads = 0;
      Object.defineProperty(png, "buffer", shadow === "data"
        ? { value: new ArrayBuffer(0) }
        : { get() { reads++; return new ArrayBuffer(0); } });
      assert.throws(() => scope.putFrame(frame({ png })), code("INVALID_INPUT"));
      assert.equal(reads, 0);
      assert.deepEqual(store.stats(), before);
      assert.deepEqual(scope.getFrame(retained.id).png, PNG);
      assert.ok(scope.putFrame(frame()));
      store.dispose();
    }
  });
}

for (const [limits, expected] of [
  [{ maxArtifactBytes: PNG.length - 1 }, "PAYLOAD_LIMIT"],
  [{ maxTotalBytes: PNG.length - 1 }, "STORAGE_LIMIT"],
]) {
  test(`${expected} cannot be bypassed by changing shadowed lengths between checks and copy`, () => {
    const { store, scope } = setup(limits);
    const before = store.stats();
    const png = Buffer.from(PNG);
    let reads = 0;
    Object.defineProperties(png, {
      length: { get() { return ++reads <= 3 ? PNG.length - 1 : PNG.length; } },
      byteLength: { get() { throw new Error("must not read instance byteLength"); } },
    });
    assert.throws(() => scope.putFrame(frame({ png })), code(expected));
    assert.equal(reads, 0);
    assert.deepEqual(store.stats(), before);
  });
}

test("unshared subviews use intrinsic bytes without invoking shadowed properties or methods", () => {
  const { store, scope } = setup({ maxArtifactBytes: PNG.length, maxTotalBytes: PNG.length });
  const backing = Buffer.alloc(PNG.length + 16, 255);
  backing.set(PNG, 8);
  const png = backing.subarray(8, 8 + PNG.length);
  let reads = 0;
  for (const key of ["buffer", "length", "byteLength", "byteOffset", "copy", "subarray", Symbol.iterator]) {
    Object.defineProperty(png, key, { get() { reads++; throw new Error("must not read instance properties"); } });
  }
  const descriptor = scope.putFrame(frame({ png }));
  assert.equal(reads, 0);
  assert.equal(descriptor.byteLength, PNG.length);
  assert.equal(descriptor.sha256, createHash("sha256").update(PNG).digest("hex"));
  assert.deepEqual(scope.getFrame(descriptor.id).png, PNG);
  backing.fill(0);
  assert.deepEqual(scope.getFrame(descriptor.id).png, PNG);
  assert.equal(store.stats().encodedBytes, PNG.length);
});

test("Buffer lookalikes and proxies fail as INVALID_INPUT without invoking traps", () => {
  const { store, scope } = setup();
  const before = store.stats();
  const forged = Object.create(Buffer.prototype, {
    buffer: { value: new ArrayBuffer(PNG.length) }, length: { value: PNG.length },
  });
  for (let index = 0; index < PNG.length; index++) forged[index] = PNG[index];
  let traps = 0;
  const proxy = new Proxy(Buffer.from(PNG), {
    get() { traps++; throw new Error("must not read a proxy"); },
    getPrototypeOf() { traps++; throw new Error("must not inspect a proxy prototype"); },
  });
  for (const png of [forged, proxy, new Uint8Array(PNG), new DataView(new ArrayBuffer(8))]) {
    assert.throws(() => scope.putFrame(frame({ png })), code("INVALID_INPUT"));
    assert.deepEqual(store.stats(), before);
  }
  assert.equal(traps, 0);
  assert.ok(scope.putFrame(frame()));
});
