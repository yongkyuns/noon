import assert from "node:assert/strict";
import { test } from "node:test";

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
