import assert from "node:assert/strict";
import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import test from "node:test";
import { deflateSync } from "node:zlib";
import { AgentPreviewService } from "../src/preview-service.mjs";

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
const BUILD_IDENTITY = Object.freeze({
  schema: 1,
  sourceRevision: "a".repeat(40),
  files: Object.freeze({
    worker: Object.freeze({ sha256: "b".repeat(64) }),
    wasm: Object.freeze({ sha256: "c".repeat(64) }),
    glue: Object.freeze({ sha256: "d".repeat(64) }),
    verifier: Object.freeze({ sha256: "e".repeat(64) }),
  }),
  buildId: "f".repeat(64),
});

function capture(time, buildIdentity = BUILD_IDENTITY) {
  return Object.freeze({
    state: "ready",
    sourceState: "running",
    authoredDuration: 4,
    frame: Object.freeze({
      requestedTime: time,
      publishedTime: time + 0.01,
      rendererBackend: "webgl2",
      objectCount: 1,
      drawCalls: 1,
    }),
    image: Object.freeze({ mimeType: "image/png", byteLength: PNG.length }),
    buildIdentity,
    imageData: Buffer.from(PNG),
  });
}

function snapshotOf(value) {
  const { imageData: _imageData, ...snapshot } = value;
  return Object.freeze(snapshot);
}

class FakeSession {
  #control;
  #snapshot = Object.freeze({ state: "new", frame: null, image: null });

  constructor(control) { this.#control = control; }
  get snapshot() { return this.#snapshot; }

  async open(source) {
    this.#control.opened.push(source);
    if (this.#control.openError) throw this.#control.openError;
    const result = capture(0, this.#control.buildIdentity);
    this.#snapshot = snapshotOf(result);
    return result;
  }

  async sample(time) {
    this.#control.sampled.push(time);
    if (this.#control.sampleBarrier) await this.#control.sampleBarrier;
    if (this.#control.sampleErrorAt === time) {
      if (this.#control.fatalSampleError) {
        this.#snapshot = Object.freeze({ ...this.#snapshot, state: "closed", error: "fixture fatal sample" });
      }
      throw new Error("fixture sample failed");
    }
    const result = capture(time, this.#control.buildIdentity);
    this.#snapshot = snapshotOf(result);
    return result;
  }

  async close(reason) {
    this.#control.closed.push(reason);
    this.#snapshot = Object.freeze({ ...this.#snapshot, state: "closed" });
    return this.#snapshot;
  }
}

function setup({ maxFramesPerSession = 32, artifactLimits = {}, session = {} } = {}) {
  const controls = [];
  const service = new AgentPreviewService({
    maxFramesPerSession,
    artifactLimits,
    createSession: () => {
      const control = {
        opened: [], sampled: [], closed: [], buildIdentity: BUILD_IDENTITY,
        openError: null, sampleErrorAt: null, fatalSampleError: false, sampleBarrier: null,
        ...session,
      };
      controls.push(control);
      return new FakeSession(control);
    },
  });
  return { service, scope: service.openScope(), controls };
}

test("open binds submitted source, observed build and actual PNG to one opaque session", async () => {
  const { service, scope, controls } = setup();
  const source = "from noon import *\nSquare()";
  const opened = await service.open(scope, source);
  assert.match(opened.sessionId, /^[0-9a-f-]{36}$/);
  assert.equal(Object.hasOwn(opened.snapshot, "imageData"), false);
  assert.equal(controls[0].opened[0], source);
  assert.deepEqual(opened.artifact.provenance.build, {
    engineRevision: "a".repeat(40),
    wasmSha256: "c".repeat(64),
    workerSha256: "b".repeat(64),
    buildId: "f".repeat(64),
  });
  assert.equal(opened.artifact.provenance.sourceSha256,
    createHash("sha256").update(source, "utf8").digest("hex"));
  assert.equal(opened.artifact.provenance.requestedTime, 0);
  assert.equal(opened.artifact.provenance.backend, "webgl2");
  assert.equal(opened.artifact.provenance.sceneRevision, null);
  assert.equal(opened.artifact.provenance.frameRevision, null);

  const first = service.getArtifact(scope, opened.sessionId, opened.artifact.id);
  assert.deepEqual(first.png, PNG);
  first.png.fill(0);
  assert.deepEqual(service.getArtifact(scope, opened.sessionId, opened.artifact.id).png, PNG);
  assert.equal(service.stats.sessions, 1);
  assert.equal(service.stats.artifacts.artifacts, 1);
});

test("batch sampling preflights ordering and retained-frame budget before advancing", async () => {
  const { service, scope, controls } = setup({ maxFramesPerSession: 4 });
  const opened = await service.open(scope, "scene()");

  await assert.rejects(service.sampleFrames(scope, opened.sessionId, [1, 0.5]), /cannot move backwards/);
  await assert.rejects(service.sampleFrames(scope, opened.sessionId, [0.5, 1, 1.5, 2]), /retained frame limit/);
  assert.deepEqual(controls[0].sampled, []);

  const frames = await service.sampleFrames(scope, opened.sessionId, [0.5, 1, 1]);
  assert.deepEqual(controls[0].sampled, [0.5, 1, 1]);
  assert.deepEqual(frames.map((entry) => entry.artifact.provenance.requestedTime), [0.5, 1, 1]);
  assert.equal(service.inspect(scope, opened.sessionId).retainedFrames, 4);
  await assert.rejects(service.sample(scope, opened.sessionId, 2), /retained frame limit/);
  assert.deepEqual(controls[0].sampled, [0.5, 1, 1]);
});

test("overlapping composition requests reject before entering the shared session", async () => {
  let releaseSample;
  const sampleBarrier = new Promise((resolve) => { releaseSample = resolve; });
  const { service, scope, controls } = setup({ session: { sampleBarrier } });
  const opened = await service.open(scope, "scene()");

  const first = service.sample(scope, opened.sessionId, 0.5);
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(controls[0].sampled, [0.5]);

  await assert.rejects(service.sample(scope, opened.sessionId, 1), /preview service operation already in progress/);
  assert.deepEqual(controls[0].sampled, [0.5]);
  assert.equal(service.inspect(scope, opened.sessionId).retainedFrames, 1);

  releaseSample();
  const completed = await first;
  assert.equal(completed.artifact.provenance.requestedTime, 0.5);
  assert.equal(service.inspect(scope, opened.sessionId).retainedFrames, 2);
  assert.equal(service.stats.sessions, 1);
});

test("missing observed build identity rejects open and retires the execution session", async () => {
  const { service, scope, controls } = setup({ session: { buildIdentity: null } });
  await assert.rejects(service.open(scope, "scene()"), /lacks observed runtime build identity/);
  assert.equal(controls.length, 1);
  assert.equal(controls[0].closed.length, 1);
  assert.equal(service.stats.sessions, 0);
  assert.equal(service.stats.artifacts.scopes, 0);
  assert.equal(service.stats.artifacts.artifacts, 0);
});

test("artifact admission failure after a sample fails closed instead of leaving advanced live state", async () => {
  const { service, scope, controls } = setup({
    maxFramesPerSession: 4,
    artifactLimits: { maxTotalBytes: PNG.length },
  });
  const opened = await service.open(scope, "scene()");
  await assert.rejects(service.sample(scope, opened.sessionId, 0.5), /retained byte limit/);
  assert.deepEqual(controls[0].sampled, [0.5]);
  assert.equal(controls[0].closed.length, 1);
  assert.equal(service.stats.sessions, 0);
  assert.equal(service.stats.artifacts.artifacts, 0);
  assert.throws(() => service.inspect(scope, opened.sessionId), /stale or cross-scope/);
});

test("fatal session failures revoke artifacts while recoverable session errors preserve them", async () => {
  const recoverable = setup({ session: { sampleErrorAt: 0.5 } });
  const a = await recoverable.service.open(recoverable.scope, "scene()");
  await assert.rejects(recoverable.service.sample(recoverable.scope, a.sessionId, 0.5), /fixture sample failed/);
  assert.equal(recoverable.service.stats.sessions, 1);
  assert.ok(recoverable.service.getArtifact(recoverable.scope, a.sessionId, a.artifact.id));
  assert.ok(await recoverable.service.sample(recoverable.scope, a.sessionId, 1));

  const fatal = setup({ session: { sampleErrorAt: 0.5, fatalSampleError: true } });
  const b = await fatal.service.open(fatal.scope, "scene()");
  await assert.rejects(fatal.service.sample(fatal.scope, b.sessionId, 0.5), /fixture sample failed/);
  assert.equal(fatal.service.stats.sessions, 0);
  assert.equal(fatal.service.stats.artifacts.artifacts, 0);
  assert.throws(() => fatal.service.getArtifact(fatal.scope, b.sessionId, b.artifact.id), /stale or cross-scope/);
});

test("session and artifact capabilities remain scope-local and close revokes both", async () => {
  const { service, scope, controls } = setup();
  const other = service.openScope();
  const opened = await service.open(scope, "scene()");
  assert.throws(() => service.inspect(other, opened.sessionId), /stale or cross-scope/);
  assert.throws(() => service.getArtifact(other, opened.sessionId, opened.artifact.id), /stale or cross-scope/);

  const closed = await service.close(scope, opened.sessionId, "fixture complete");
  assert.equal(closed.state, "closed");
  assert.equal(controls[0].closed.length, 1);
  assert.equal(service.stats.sessions, 0);
  assert.equal(service.stats.artifacts.artifacts, 0);
  assert.throws(() => service.inspect(scope, opened.sessionId), /stale or cross-scope/);
});

test("closeScope and dispose await session cleanup and release every retained artifact", async () => {
  const { service, scope, controls } = setup();
  const first = await service.open(scope, "first()");
  const second = await service.open(scope, "second()");
  assert.notEqual(first.sessionId, second.sessionId);
  assert.equal(service.stats.artifacts.artifacts, 2);

  await service.closeScope(scope, "transport gone");
  assert.equal(controls[0].closed.length, 1);
  assert.equal(controls[1].closed.length, 1);
  assert.equal(service.stats.sessions, 0);
  assert.equal(service.stats.artifacts.artifacts, 0);
  assert.throws(() => service.inspect(scope, first.sessionId), /stale preview service scope/);

  const next = service.openScope();
  await service.open(next, "third()");
  await service.dispose("fixture shutdown");
  assert.equal(controls[2].closed.length, 1);
  assert.equal(service.stats.disposed, true);
  assert.equal(service.stats.artifacts.disposed, true);
  assert.throws(() => service.openScope(), /preview service is disposed/);
});
