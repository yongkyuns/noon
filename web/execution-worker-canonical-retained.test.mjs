import assert from "node:assert/strict";
import test from "node:test";

class FakeCanvas {
  clientWidth = 640;
  clientHeight = 360;
  width = 640;
  height = 360;
  transferred = false;
  replacement = null;

  transferControlToOffscreen() {
    if (this.transferred) {
      throw new Error("canvas was transferred twice");
    }
    this.transferred = true;
    return { width: this.width, height: this.height };
  }

  cloneNode() {
    const clone = new FakeCanvas();
    clone.clientWidth = this.clientWidth;
    clone.clientHeight = this.clientHeight;
    clone.width = this.width;
    clone.height = this.height;
    return clone;
  }

  replaceWith(replacement) {
    this.replacement = replacement;
  }
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
}

globalThis.HTMLCanvasElement = FakeCanvas;
globalThis.Worker = FakeWorker;
globalThis.window = { devicePixelRatio: 1 };

const { ExecutionWorkerClient } = await import("./execution-worker-client.js");

const SCENE_SPEC_JSON = JSON.stringify({
  version: 1,
  camera_object: null,
  objects: [{ id: 7, content: { kind: "text", text: {} } }],
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

function retainedEngines() {
  return FakeWorker.instances.filter(({ name }) => name === "noon-mixed-retained-engine");
}

function latestRender() {
  return FakeWorker.instances.findLast(({ name }) => name === "noon-render");
}

function latestMessage(worker, type) {
  return worker.messages.findLast(({ message }) => message.type === type)?.message;
}

function emitReady(engine, render) {
  engine.emitMessage(
    engineMessage("ready", {
      transportMode: "transferable",
      retained: true,
      mixed: true,
      canonical: true,
    }),
  );
  render.emitMessage(
    renderMessage("ready", {
      transportMode: "transferable",
      backend: "WebGL2",
      retained: true,
      mixed: true,
    }),
  );
}

test("canonical retained authoring survives engine-only and full recovery", async () => {
  FakeWorker.instances.length = 0;
  const client = new ExecutionWorkerClient(new FakeCanvas());
  const started = client.startRetainedCanonical(SCENE_SPEC_JSON, {
    transportMode: "transferable",
  });
  const firstEngine = retainedEngines()[0];
  const firstRender = latestRender();
  const firstInit = latestMessage(firstEngine, "init");
  assert.equal(firstInit.sceneSpecJson, SCENE_SPEC_JSON);
  assert.equal("sceneJson" in firstInit, false);
  assert.equal("retainedDocumentJson" in firstInit, false);
  emitReady(firstEngine, firstRender);
  assert.equal((await started).session, 1);

  const engineRestart = client.restart({ failedOwner: "engine" });
  const secondEngine = retainedEngines()[1];
  const attach = latestMessage(firstRender, "attach_engine");
  const secondInit = latestMessage(secondEngine, "init");
  assert.equal(secondInit.sceneSpecJson, SCENE_SPEC_JSON);
  assert.equal("sceneJson" in secondInit, false);
  assert.equal("retainedDocumentJson" in secondInit, false);
  secondEngine.emitMessage(
    engineMessage("ready", {
      transportMode: "transferable",
      retained: true,
      mixed: true,
      canonical: true,
    }),
  );
  firstRender.emitMessage(
    renderMessage("engine_attached", {
      requestId: attach.requestId,
      transportMode: "transferable",
      backend: "WebGL2",
      retained: true,
      mixed: true,
    }),
  );
  const reconnected = await engineRestart;
  assert.equal(reconnected.session, 2);
  assert.equal(reconnected.engine.canonical, true);

  const beforeFullRecoveryCanvas = client.canvas;
  const fullRestart = client.restart({ failedOwner: "render" });
  const thirdEngine = retainedEngines()[2];
  const secondRender = latestRender();
  const thirdInit = latestMessage(thirdEngine, "init");
  assert.notEqual(client.canvas, beforeFullRecoveryCanvas);
  assert.equal(thirdInit.sceneSpecJson, SCENE_SPEC_JSON);
  assert.equal("sceneJson" in thirdInit, false);
  assert.equal("retainedDocumentJson" in thirdInit, false);
  emitReady(thirdEngine, secondRender);
  const recovered = await fullRestart;
  assert.equal(recovered.session, 3);
  assert.equal(recovered.engine.canonical, true);

  client.terminate();
});