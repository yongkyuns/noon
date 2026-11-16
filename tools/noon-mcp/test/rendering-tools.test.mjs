import assert from "node:assert/strict";
import { test } from "node:test";
import { deflateSync } from "node:zlib";

import { FrameArtifactStore } from "../../../scripts/agent-preview-artifacts.mjs";
import { AgentPreviewService } from "../src/preview-service.mjs";
import { RENDERING_TOOL_NAMES, renderingToolContracts } from "../src/rendering-contract.mjs";
import { registerRenderingTools } from "../src/rendering-tools.mjs";

const scope = Object.freeze(Object.create(null));
const sessionId = "01234567-89ab-cdef-0123-456789abcdef";
const png0 = Buffer.from("frame-zero");
const png1 = Buffer.from("frame-one");
const png2 = Buffer.from("frame-two");
const descriptor = (id, time) => Object.freeze({
  id,
  mimeType: "image/png",
  sha256: String(time).padStart(64, "0"),
  byteLength: 10,
  provenance: Object.freeze({ sessionId, requestedTime: time }),
});
const snapshot = (time) => Object.freeze({ state: "ready", frame: Object.freeze({ requestedTime: time, publishedTime: time }) });

function fakeServer() {
  const tools = new Map();
  return {
    tools,
    registerTool(name, config, callback) {
      assert.equal(tools.has(name), false, `duplicate tool ${name}`);
      tools.set(name, { config, callback });
    },
  };
}

function fakeService() {
  const calls = [];
  const artifacts = new Map([
    ["a0", Object.freeze({ descriptor: descriptor("a0", 0), png: Buffer.from(png0) })],
    ["a1", Object.freeze({ descriptor: descriptor("a1", 1), png: Buffer.from(png1) })],
    ["a2", Object.freeze({ descriptor: descriptor("a2", 2), png: Buffer.from(png2) })],
  ]);
  return {
    calls,
    async open(actualScope, source, options) {
      calls.push(["open", actualScope, source, options]);
      return Object.freeze({ sessionId, snapshot: snapshot(0), artifact: artifacts.get("a0").descriptor });
    },
    async sampleFrames(actualScope, actualSession, times, options) {
      calls.push(["sampleFrames", actualScope, actualSession, [...times], options]);
      return Object.freeze(times.map((time, index) => Object.freeze({
        snapshot: snapshot(time), artifact: artifacts.get(index === 0 ? "a1" : "a2").descriptor,
      })));
    },
    inspect(actualScope, actualSession) {
      calls.push(["inspect", actualScope, actualSession]);
      return Object.freeze({ snapshot: snapshot(2), artifact: artifacts.get("a2").descriptor, retainedFrames: 3, maxRetainedFrames: 32 });
    },
    getArtifact(actualScope, actualSession, id) {
      calls.push(["getArtifact", actualScope, actualSession, id]);
      return artifacts.get(id);
    },
    async close(actualScope, actualSession, reason) {
      calls.push(["close", actualScope, actualSession, reason]);
      return Object.freeze({ state: "closed" });
    },
  };
}

function context(signal = new AbortController().signal) {
  return Object.freeze({ mcpReq: Object.freeze({ signal }) });
}

// Genuine tiny PNG test data. It is not a captured Noon render.
function pngChunk(kind, bytes = Buffer.alloc(0)) {
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

function feedbackPng() {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(2);
  header.writeUInt32BE(1, 4);
  header[8] = 8;
  header[9] = 6;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk("IHDR", header), pngChunk("IDAT", deflateSync(Buffer.alloc(9))), pngChunk("IEND")]);
}

const feedbackImage = feedbackPng();
const buildIdentity = Object.freeze({
  sourceRevision: "a".repeat(40),
  files: Object.freeze({
    worker: Object.freeze({ sha256: "b".repeat(64) }),
    wasm: Object.freeze({ sha256: "c".repeat(64) }),
  }),
  buildId: "d".repeat(64),
});
const feedbackFields = Object.freeze({
  interval: Object.freeze({ start: 0, end: 0.5 }),
  region: Object.freeze({ x: 0.25, y: 0, width: 0.5, height: 1 }),
  observation: "The sum is zero but the input arrows have disappeared.",
  desiredChange: "Keep both input arrows visible as the sum changes.",
  preserve: "The two contributions remain present.",
});

function ownedPreview(t) {
  const calls = [];
  const service = new AgentPreviewService({
    createSession() {
      let current = Object.freeze({ state: "new", frame: null, image: null });
      function capture(time) {
        current = Object.freeze({
          state: "ready", sourceState: "running", authoredDuration: 4,
          frame: Object.freeze({ requestedTime: time, publishedTime: time + 0.01, rendererBackend: "webgl2" }),
          image: Object.freeze({ mimeType: "image/png", byteLength: feedbackImage.length }),
          buildIdentity,
        });
        return Object.freeze({ ...current, imageData: Buffer.from(feedbackImage) });
      }
      return {
        get snapshot() { return current; },
        async open(source) { calls.push(["open", source]); return capture(0); },
        async sample(time) { calls.push(["sample", time]); return capture(time); },
        async close(reason) {
          calls.push(["close", reason]);
          current = Object.freeze({ ...current, state: "closed" });
          return current;
        },
      };
    },
  });
  t.after(() => service.dispose());
  const ownedScope = service.openScope();
  const server = fakeServer();
  registerRenderingTools(server, { service, scope: ownedScope });
  return { service, scope: ownedScope, server, calls };
}

test("registers only the prepared rendering contracts on the existing server", () => {
  const server = fakeServer();
  const service = fakeService();
  assert.equal(registerRenderingTools(server, { service, scope }), RENDERING_TOOL_NAMES);
  assert.deepEqual([...server.tools.keys()], RENDERING_TOOL_NAMES);
  for (const name of RENDERING_TOOL_NAMES) assert.equal(server.tools.get(name).config, renderingToolContracts[name]);
  assert.throws(() => registerRenderingTools({}, { service, scope }), /existing server/);
  assert.throws(() => registerRenderingTools(server, { service: {}, scope }), /shared preview service/);
  assert.throws(() => registerRenderingTools(server, { service, scope: null }), /transport-owned preview scope/);
});

test("open_scene returns the retained PNG and propagates request cancellation", async () => {
  const server = fakeServer();
  const service = fakeService();
  registerRenderingTools(server, { service, scope });
  const controller = new AbortController();
  const result = await server.tools.get("noon_open_scene").callback(
    { source: "from noon import *\n", loopDurationSeconds: 4 }, context(controller.signal));

  assert.notEqual(result.isError, true);
  assert.equal(result.structuredContent.session, sessionId);
  assert.equal(result.structuredContent.artifact.id, "a0");
  assert.deepEqual(result.content.map((item) => item.type), ["text", "image"]);
  assert.equal(result.content[1].mimeType, "image/png");
  assert.equal(result.content[1].data, png0.toString("base64"));
  assert.deepEqual(JSON.parse(result.content[0].text), result.structuredContent);
  assert.equal(service.calls[0][0], "open");
  assert.equal(service.calls[0][1], scope);
  assert.equal(service.calls[0][3].signal, controller.signal);
  assert.deepEqual(service.calls.map((entry) => entry[0]), ["open", "getArtifact"]);
});

test("sample_frames preserves frame/image order and scoped artifact lookup", async () => {
  const server = fakeServer();
  const service = fakeService();
  registerRenderingTools(server, { service, scope });
  const controller = new AbortController();
  const result = await server.tools.get("noon_sample_frames").callback(
    { session: sessionId, times: [1, 2] }, context(controller.signal));

  assert.notEqual(result.isError, true);
  assert.equal(result.structuredContent.session, sessionId);
  assert.deepEqual(result.structuredContent.frames.map((frame) => frame.artifact.id), ["a1", "a2"]);
  assert.deepEqual(result.content.map((item) => item.type), ["text", "image", "image"]);
  assert.equal(result.content[1].data, png1.toString("base64"));
  assert.equal(result.content[2].data, png2.toString("base64"));
  assert.equal(service.calls[0][0], "sampleFrames");
  assert.equal(service.calls[0][1], scope);
  assert.equal(service.calls[0][2], sessionId);
  assert.equal(service.calls[0][4].signal, controller.signal);
  assert.deepEqual(service.calls.slice(1).map((entry) => entry[3]), ["a1", "a2"]);
});

test("inspect is metadata-only and close delegates stale-handle ownership to the service", async () => {
  const server = fakeServer();
  const service = fakeService();
  registerRenderingTools(server, { service, scope });

  const inspected = await server.tools.get("noon_inspect").callback({ session: sessionId }, context());
  assert.deepEqual(inspected.content.map((item) => item.type), ["text"]);
  assert.equal(inspected.structuredContent.artifact.id, "a2");
  assert.equal(inspected.structuredContent.retainedFrames, 3);

  const closed = await server.tools.get("noon_close_scene").callback({ session: sessionId }, context());
  assert.equal(closed.structuredContent.session, sessionId);
  assert.equal(closed.structuredContent.closed.state, "closed");
  assert.deepEqual(service.calls.slice(-2).map((entry) => entry[0]), ["inspect", "close"]);
  assert.equal(service.calls.at(-1)[3], "MCP close_scene");
});

test("operation failures remain structured and do not leak stacks", async () => {
  const server = fakeServer();
  const service = fakeService();
  service.sampleFrames = async () => {
    const error = new Error("retained frame count limit reached");
    error.name = "ArtifactError";
    error.code = "FRAME_LIMIT";
    error.stack = "SECRET STACK";
    throw error;
  };
  registerRenderingTools(server, { service, scope });
  const result = await server.tools.get("noon_sample_frames").callback({ session: sessionId, times: [1] }, context());
  assert.equal(result.isError, true);
  assert.deepEqual(result.structuredContent, {
    error: { name: "ArtifactError", code: "FRAME_LIMIT", message: "retained frame count limit reached" },
  });
  assert.deepEqual(JSON.parse(result.content[0].text), result.structuredContent);
  assert.doesNotMatch(result.content[0].text, /SECRET STACK/);
});

test("unreadable open image retires the undisclosed session before returning an error", async () => {
  const server = fakeServer();
  const service = fakeService();
  service.getArtifact = () => Object.freeze({ descriptor: descriptor("different", 0), png: Buffer.from(png0) });
  registerRenderingTools(server, { service, scope });
  const result = await server.tools.get("noon_open_scene").callback({ source: "scene" }, context());
  assert.equal(result.isError, true);
  assert.match(result.structuredContent.error.message, /readable retained PNG artifact/);
  assert.deepEqual(result.content.map((item) => item.type), ["text"]);
  assert.deepEqual(service.calls.map((entry) => entry[0]), ["open", "close"]);
  assert.equal(service.calls.at(-1)[3], "MCP open_scene image delivery failed");
});

test("unreadable sampled image retires the advanced session before returning an error", async () => {
  const server = fakeServer();
  const service = fakeService();
  const getArtifact = service.getArtifact.bind(service);
  service.getArtifact = (actualScope, actualSession, id) => {
    if (id === "a2") return Object.freeze({ descriptor: descriptor("different", 2), png: Buffer.from(png2) });
    return getArtifact(actualScope, actualSession, id);
  };
  registerRenderingTools(server, { service, scope });
  const result = await server.tools.get("noon_sample_frames").callback({ session: sessionId, times: [1, 2] }, context());
  assert.equal(result.isError, true);
  assert.match(result.structuredContent.error.message, /readable retained PNG artifact/);
  assert.deepEqual(result.content.map((item) => item.type), ["text"]);
  assert.deepEqual(service.calls.map((entry) => entry[0]), ["sampleFrames", "getArtifact", "close"]);
  assert.equal(service.calls.at(-1)[3], "MCP sample_frames image delivery failed");
});

test("annotation returns pending feedback and its owned historical frame without advancing or closing", async (t) => {
  const { service, scope: ownedScope, server, calls } = ownedPreview(t);
  const opened = await service.open(ownedScope, "from noon import *\nSquare()");
  await service.sampleFrames(ownedScope, opened.sessionId, [2]);
  const before = service.inspect(ownedScope, opened.sessionId);
  const stats = service.stats;
  const lookups = [];
  const read = service.getArtifact.bind(service);
  service.getArtifact = (...args) => { lookups.push(args); return read(...args); };
  const input = { session: opened.sessionId, artifactId: opened.artifact.id, ...feedbackFields };
  const annotate = server.tools.get("noon_annotate_frame").callback;
  const result = await annotate(input, context());

  assert.notEqual(result.isError, true);
  assert.deepEqual(lookups, [[ownedScope, opened.sessionId, opened.artifact.id]]);
  assert.deepEqual(result.content.map((item) => item.type), ["text", "image"]);
  assert.equal(result.content[1].data, feedbackImage.toString("base64"));
  assert.deepEqual(JSON.parse(result.content[0].text), result.structuredContent);
  const record = result.structuredContent.feedback;
  assert.equal(record.kind, "noon-frame-feedback");
  assert.equal(record.schemaVersion, 1);
  assert.equal(record.status, "pending_review");
  assert.deepEqual(record.feedback, feedbackFields);
  assert.deepEqual(record.frame.provenance, opened.artifact.provenance);
  assert.equal(record.frame.id, opened.artifact.id);
  assert.equal(record.frame.sha256, opened.artifact.sha256);
  assert.equal(record.frame.width, 2);
  assert.equal(record.frame.height, 1);
  assert.equal(Object.hasOwn(record.frame, "retentionMs"), false);
  assert.ok(Object.isFrozen(record));
  assert.ok(Object.isFrozen(record.frame.provenance));
  assert.deepEqual(service.inspect(ownedScope, opened.sessionId), before);
  assert.deepEqual(service.stats, stats);
  assert.deepEqual(calls.map(([name]) => name), ["open", "sample"]);
  const repeated = await annotate(input, context());
  assert.deepEqual(repeated.structuredContent, result.structuredContent);
});

test("annotation rejects forged input before reading an artifact", async () => {
  const server = fakeServer();
  const service = fakeService();
  registerRenderingTools(server, { service, scope });
  const annotate = server.tools.get("noon_annotate_frame").callback;
  for (const extra of [
    { status: "resolved" },
    { provenance: { sessionId, sourceSha256: "f".repeat(64) } },
    { interval: { start: 2, end: 1 } },
  ]) {
    const result = await annotate({
      session: sessionId, artifactId: "artifact_0123456789abcdef", ...feedbackFields, ...extra,
    }, context());
    assert.equal(result.isError, true);
    assert.deepEqual(result.content.map((item) => item.type), ["text"]);
  }
  assert.deepEqual(service.calls, []);
});

test("annotation rejects mismatched artifact or session and corrupt evidence without retiring the session", async (t) => {
  const { service, scope: ownedScope, server, calls } = ownedPreview(t);
  const opened = await service.open(ownedScope, "scene()");
  const retained = service.getArtifact(ownedScope, opened.sessionId, opened.artifact.id);
  const annotate = server.tools.get("noon_annotate_frame").callback;
  const input = { session: opened.sessionId, artifactId: opened.artifact.id, ...feedbackFields };
  const badEvidence = [
    { ...retained, descriptor: { ...retained.descriptor, id: "another-artifact-id" } },
    { ...retained, descriptor: { ...retained.descriptor,
      provenance: { ...retained.descriptor.provenance, sessionId: "another-session-id" } } },
    { ...retained, png: Buffer.from("corrupt evidence") },
  ];
  for (const evidence of badEvidence) {
    let reads = 0;
    service.getArtifact = (...args) => {
      reads += 1;
      assert.deepEqual(args, [ownedScope, opened.sessionId, opened.artifact.id]);
      return evidence;
    };
    const result = await annotate(input, context());
    assert.equal(result.isError, true);
    assert.equal(reads, 1);
    assert.deepEqual(result.content.map((item) => item.type), ["text"]);
    assert.equal(service.inspect(ownedScope, opened.sessionId).snapshot.state, "ready");
  }
  assert.deepEqual(calls.map(([name]) => name), ["open"]);
});

test("annotation preserves real service ownership and closed-session rejection", async (t) => {
  const { service, scope: ownedScope, server, calls } = ownedPreview(t);
  const opened = await service.open(ownedScope, "first_scene()");
  const other = await service.open(ownedScope, "second_scene()");
  const foreignScope = service.openScope();
  const foreignServer = fakeServer();
  registerRenderingTools(foreignServer, { service, scope: foreignScope });
  const input = { session: opened.sessionId, artifactId: opened.artifact.id, ...feedbackFields };
  const foreign = await foreignServer.tools.get("noon_annotate_frame").callback(input, context());
  assert.equal(foreign.isError, true);
  assert.match(foreign.structuredContent.error.message, /stale or cross-scope/);

  const annotate = server.tools.get("noon_annotate_frame").callback;
  const wrongSession = await annotate({ ...input, artifactId: other.artifact.id }, context());
  assert.equal(wrongSession.isError, true);
  assert.equal(wrongSession.structuredContent.error.code, "ARTIFACT_NOT_FOUND");
  assert.deepEqual(calls.map(([name]) => name), ["open", "open"]);

  await service.close(ownedScope, opened.sessionId, "review session complete");
  const closed = await annotate(input, context());
  assert.equal(closed.isError, true);
  assert.match(closed.structuredContent.error.message, /stale or cross-scope/);
  assert.deepEqual(calls.map(([name]) => name), ["open", "open", "close"]);
  assert.equal(service.inspect(ownedScope, other.sessionId).snapshot.state, "ready");
});

test("expired frame feedback returns the artifact error without closing or recreating evidence", async (t) => {
  let now = 0;
  const store = new FrameArtifactStore({ clock: () => now, limits: { ttlMs: 100 } });
  t.after(() => store.dispose());
  const artifactScope = store.openScope({
    sessionId, sourceSha256: "e".repeat(64),
    build: { engineRevision: buildIdentity.sourceRevision,
      workerSha256: buildIdentity.files.worker.sha256, wasmSha256: buildIdentity.files.wasm.sha256,
      buildId: buildIdentity.buildId },
  });
  const artifact = artifactScope.putFrame({
    png: feedbackImage, requestedTime: 0, publishedTime: 0.01,
    backend: "webgl2", sceneRevision: null, frameRevision: null,
  });
  const server = fakeServer();
  const service = fakeService();
  service.getArtifact = (actualScope, actualSession, id) => {
    service.calls.push(["getArtifact", actualScope, actualSession, id]);
    assert.equal(actualScope, scope);
    assert.equal(actualSession, sessionId);
    return artifactScope.getFrame(id);
  };
  registerRenderingTools(server, { service, scope });
  now = 100;
  const result = await server.tools.get("noon_annotate_frame").callback({
    session: sessionId, artifactId: artifact.id, ...feedbackFields,
  }, context());
  assert.equal(result.isError, true);
  assert.equal(result.structuredContent.error.code, "ARTIFACT_NOT_FOUND");
  assert.deepEqual(result.content.map((item) => item.type), ["text"]);
  assert.deepEqual(service.calls.map(([name]) => name), ["getArtifact"]);
  assert.equal(store.stats().artifacts, 0);
});
