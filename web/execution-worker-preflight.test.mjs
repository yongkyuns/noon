import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("engine ready is emitted only after the complete initial render bootstrap is queued", async () => {
  const legacy = await readFile(new URL("./execution-engine-worker.js", import.meta.url), "utf8");
  const retained = await readFile(
    new URL("./retained-execution-engine-worker.js", import.meta.url),
    "utf8",
  );

  const legacySnapshot = legacy.indexOf("sendDeltaOrThrow(initial)");
  const legacyReady = legacy.indexOf('postMain({ type: "ready", transportMode })');
  assert.ok(legacySnapshot >= 0 && legacyReady > legacySnapshot);

  const retainedResources = retained.indexOf('type: "retained_resources"');
  const retainedSnapshot = retained.indexOf("sendDeltaOrThrow(player.initialDeltaJson())");
  const retainedReady = retained.indexOf('type: "ready"', retainedSnapshot);
  assert.ok(retainedResources >= 0);
  assert.ok(retainedSnapshot > retainedResources);
  assert.ok(retainedReady > retainedSnapshot);
});

class FakeCanvas {
  clientWidth = 640;
  clientHeight = 360;
  width = 640;
  height = 360;

  transferControlToOffscreen() {
    return { width: this.width, height: this.height };
  }

  cloneNode() {
    return new FakeCanvas();
  }

  replaceWith() {}
}

class FakeWorker {
  static instances = [];

  listeners = new Map();
  messages = [];
  terminated = false;

  constructor(_url, options = {}) {
    this.name = options.name ?? "";
    FakeWorker.instances.push(this);
  }

  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  postMessage(message, transfer = []) {
    this.messages.push({ message, transfer });
  }

  terminate() {
    this.terminated = true;
  }

  emitMessage(message) {
    for (const listener of this.listeners.get("message") ?? []) {
      listener({ data: message });
    }
  }

  emitError(message = "worker crashed") {
    for (const listener of this.listeners.get("error") ?? []) {
      listener({ message });
    }
  }
}

globalThis.HTMLCanvasElement = FakeCanvas;
globalThis.Worker = FakeWorker;
globalThis.window = { devicePixelRatio: 1 };

const { ExecutionWorkerClient } = await import("./execution-worker-client.js");

const SCENE_JSON = JSON.stringify({ version: 1, objects: [], tracks: [] });
const SCENE_SPEC_JSON = JSON.stringify({
  version: 1,
  camera_object: null,
  objects: [{ id: 1, content: { kind: "text", text: {} } }],
  tracks: [],
});

function engineMessage(type, payload = {}) {
  return {
    channel: "noon.engine",
    protocolVersion: 1,
    type,
    ...payload,
  };
}

function renderMessage(type, payload = {}) {
  return {
    channel: "noon.render",
    protocolVersion: 1,
    type,
    ...payload,
  };
}

function latestWorker(name) {
  return FakeWorker.instances.findLast((worker) => worker.name === name);
}

function requestMessage(worker, type) {
  const entry = worker.messages.findLast(({ message }) => message.type === type);
  assert.ok(entry, `missing ${worker.name} ${type} request`);
  return entry.message;
}

async function flush() {
  await Promise.resolve();
  await Promise.resolve();
}

async function startLegacyClient() {
  const client = new ExecutionWorkerClient(new FakeCanvas());
  const readyPromise = client.start(SCENE_JSON, { transportMode: "transferable" });
  const engine = latestWorker("noon-engine");
  const render = latestWorker("noon-render");
  assert.ok(engine);
  assert.ok(render);
  engine.emitMessage(engineMessage("ready", { transportMode: "transferable" }));
  render.emitMessage(
    renderMessage("ready", { transportMode: "transferable", backend: "WebGL2" }),
  );
  await readyPromise;
  return { client, engine, render };
}

test("mode transition preflights the candidate before touching the live render owner", async () => {
  const { client, engine: oldEngine, render } = await startLegacyClient();
  const canvas = client.canvas;

  const transition = client.switchToRetainedCanonical(SCENE_SPEC_JSON);
  await flush();
  const candidate = latestWorker("noon-mixed-retained-engine");
  assert.ok(candidate, "retained candidate must be created during preflight");
  assert.equal(oldEngine.terminated, false, "old engine must stay live during candidate preflight");
  assert.equal(
    render.messages.some(({ message }) => message.type === "switch_engine"),
    false,
    "render owner must not switch before candidate readiness",
  );
  const init = requestMessage(candidate, "init");
  assert.equal(init.session, 2, "candidate may reserve the next session without publishing it");
  assert.equal(init.sceneSpecJson, SCENE_SPEC_JSON);
  assert.equal("sceneJson" in init, false);
  assert.equal("retainedDocumentJson" in init, false);

  const oldStatePromise = client.state();
  await flush();
  const oldState = requestMessage(oldEngine, "state");
  oldEngine.emitMessage(
    engineMessage("state", {
      requestId: oldState.requestId,
      time: 0,
      playing: true,
      nextPatchSequence: "0",
      sceneJson: SCENE_JSON,
    }),
  );
  assert.equal((await oldStatePromise).sceneJson, SCENE_JSON);

  candidate.emitMessage(
    engineMessage("ready", {
      transportMode: "transferable",
      retained: true,
      mixed: true,
      canonical: true,
    }),
  );
  await flush();

  assert.equal(oldEngine.terminated, true, "old engine retires only after candidate readiness");
  const renderSwitch = requestMessage(render, "switch_engine");
  assert.equal(renderSwitch.mode, "retained");
  render.emitMessage(
    renderMessage("mode_switched", {
      requestId: renderSwitch.requestId,
      mode: "retained",
      retained: true,
      mixed: true,
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );

  const ready = await transition;
  assert.equal(ready.session, 2);
  assert.equal(client.mode, "retained");
  assert.equal(client.canvas, canvas);
  client.terminate();
});

test("candidate failure before readiness leaves the old engine and renderer authoritative", async () => {
  const { client, engine: oldEngine, render } = await startLegacyClient();
  const canvas = client.canvas;

  const transition = client.switchToRetainedCanonical(SCENE_SPEC_JSON);
  await flush();
  const candidate = latestWorker("noon-mixed-retained-engine");
  candidate.emitMessage(engineMessage("error", { message: "candidate bootstrap rejected" }));

  await assert.rejects(transition, /candidate bootstrap rejected/);
  assert.equal(candidate.terminated, true);
  assert.equal(oldEngine.terminated, false);
  assert.equal(render.terminated, false);
  assert.equal(client.mode, "legacy");
  assert.equal(client.canvas, canvas);
  assert.equal(
    render.messages.some(({ message }) => message.type === "switch_engine"),
    false,
    "failed preflight must not mutate the render owner",
  );

  const statePromise = client.state();
  await flush();
  const state = requestMessage(oldEngine, "state");
  oldEngine.emitMessage(
    engineMessage("state", {
      requestId: state.requestId,
      time: 0,
      playing: true,
      nextPatchSequence: "0",
      sceneJson: SCENE_JSON,
    }),
  );
  assert.equal((await statePromise).sceneJson, SCENE_JSON);
  client.terminate();
});

test("terminate cancels an unpublished candidate without resurrecting either generation", async () => {
  const { client, engine: oldEngine, render } = await startLegacyClient();

  const transition = client.switchToRetainedCanonical(SCENE_SPEC_JSON);
  await flush();
  const candidate = latestWorker("noon-mixed-retained-engine");
  assert.ok(candidate);

  client.terminate();
  await assert.rejects(
    transition,
    /execution worker client was terminated during an asynchronous operation/,
  );
  assert.equal(candidate.terminated, true);
  assert.equal(oldEngine.terminated, true);
  assert.equal(render.terminated, true);
  await assert.rejects(client.state(), /ExecutionWorkerClient has not been started/);
});
