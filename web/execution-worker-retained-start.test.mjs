import assert from "node:assert/strict";
import test from "node:test";

class FakeCanvas {
  clientWidth = 640;
  clientHeight = 360;
  width = 640;
  height = 360;

  transferControlToOffscreen() {
    return { width: this.width, height: this.height };
  }
}

class FakeWorker {
  static instances = [];

  listeners = new Map();
  messages = [];

  constructor(url, options = {}) {
    this.url = String(url);
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

  terminate() {}

  emitMessage(message) {
    for (const listener of this.listeners.get("message") ?? []) {
      listener({ data: message });
    }
  }
}

globalThis.HTMLCanvasElement = FakeCanvas;
globalThis.Worker = FakeWorker;
globalThis.window = { devicePixelRatio: 1 };

const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");

const SCENE_JSON = JSON.stringify({ version: 1, objects: [], tracks: [] });
const SCENE_SPEC_JSON = JSON.stringify({
  version: 1,
  camera_object: null,
  objects: [{ id: 7, content: { kind: "text", text: {} } }],
  tracks: [],
});
const RETAINED_DOCUMENT_JSON = JSON.stringify({
  channel: "noon.authoring.retained",
  objects: [{ id: 1 }],
});

function engineReady({ canonical = false } = {}) {
  return {
    channel: "noon.engine",
    protocolVersion: 1,
    type: "ready",
    transportMode: "transferable",
    retained: true,
    mixed: true,
    canonical,
  };
}

function renderReady() {
  return {
    channel: "noon.render",
    protocolVersion: 1,
    type: "ready",
    transportMode: "transferable",
    backend: "WebGL2",
  };
}

test("canonical retained first start sends only SceneSpec to the retained engine", async () => {
  FakeWorker.instances.length = 0;
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const readyPromise = client.startRetainedCanonical(SCENE_SPEC_JSON, {
    transportMode: "transferable",
  });

  const retainedEngine = FakeWorker.instances.find(
    ({ name }) => name === "noon-mixed-retained-engine",
  );
  const legacyEngine = FakeWorker.instances.find(({ name }) => name === "noon-engine");
  const render = FakeWorker.instances.find(({ name }) => name === "noon-render");

  assert.ok(retainedEngine, "retained startup must create the retained engine");
  assert.equal(legacyEngine, undefined, "retained startup must not create a legacy engine");
  assert.ok(render, "retained startup must create the shared render owner");
  assert.match(retainedEngine.url, /retained-execution-engine-worker\.js$/);

  const engineInit = retainedEngine.messages.find(({ message }) => message.type === "init")?.message;
  const renderInit = render.messages.find(({ message }) => message.type === "init")?.message;
  assert.equal(engineInit?.sceneSpecJson, SCENE_SPEC_JSON);
  assert.equal("sceneJson" in engineInit, false);
  assert.equal("retainedDocumentJson" in engineInit, false);
  assert.equal(renderInit?.mode, "retained");

  retainedEngine.emitMessage(engineReady({ canonical: true }));
  render.emitMessage(renderReady());
  const ready = await readyPromise;

  assert.equal(ready.session, 1);
  assert.equal(ready.engine.canonical, true);
  assert.equal(client.mode, "retained");
  client.terminate();
});

test("split retained startup remains an explicit compatibility path", async () => {
  FakeWorker.instances.length = 0;
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const readyPromise = client.startRetained(SCENE_JSON, RETAINED_DOCUMENT_JSON, {
    transportMode: "transferable",
  });

  const retainedEngine = FakeWorker.instances.find(
    ({ name }) => name === "noon-mixed-retained-engine",
  );
  const render = FakeWorker.instances.find(({ name }) => name === "noon-render");
  const engineInit = retainedEngine.messages.find(({ message }) => message.type === "init")?.message;
  assert.equal(engineInit?.sceneJson, SCENE_JSON);
  assert.equal(engineInit?.retainedDocumentJson, RETAINED_DOCUMENT_JSON);
  assert.equal("sceneSpecJson" in engineInit, false);

  retainedEngine.emitMessage(engineReady());
  render.emitMessage(renderReady());
  const ready = await readyPromise;
  assert.equal(ready.session, 1);
  assert.equal(client.mode, "retained");
  client.terminate();
});