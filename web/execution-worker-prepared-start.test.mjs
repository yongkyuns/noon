import assert from "node:assert/strict";
import test from "node:test";

class FakeCanvas {
  clientWidth = 640;
  clientHeight = 360;
  width = 640;
  height = 360;
  replacement = null;
  transferred = false;

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
  static failNextName = null;

  listeners = new Map();
  messages = [];
  terminated = false;

  constructor(url, options = {}) {
    this.url = String(url);
    this.name = options.name ?? "";
    if (FakeWorker.failNextName === this.name) {
      FakeWorker.failNextName = null;
      throw new Error(`${this.name} constructor failed`);
    }
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
    this.#emit("message", { data: message });
  }

  emitError(message = "worker crashed") {
    this.#emit("error", { message });
  }

  #emit(type, event) {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
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

function requestMessage(worker, type) {
  const entry = worker.messages.findLast(({ message }) => message.type === type);
  assert.ok(entry, `missing ${worker.name} ${type} request`);
  return entry.message;
}

function workerByName(offset, name) {
  const worker = FakeWorker.instances.slice(offset).find((candidate) => candidate.name === name);
  assert.ok(worker, `missing worker ${name}`);
  return worker;
}

function acknowledgePreparation(render) {
  const request = requestMessage(render, "prepare");
  render.emitMessage(
    renderMessage("prepared", {
      requestId: request.requestId,
      transportMode: "transferable",
      width: 640,
      height: 360,
    }),
  );
  return request;
}

function resetWorkers() {
  FakeWorker.instances.length = 0;
  FakeWorker.failNextName = null;
}

test("prepared render owner waits for preparation before starting the authored retained engine", async () => {
  resetWorkers();
  const client = new ExecutionWorkerClient(new FakeCanvas());

  const preparePromise = client.prepare({ transportMode: "transferable" });
  const render = workerByName(0, "noon-render");
  assert.equal(
    FakeWorker.instances.some(({ name }) => name === "noon-engine"),
    false,
    "prepare must not speculate a legacy engine",
  );
  assert.equal(
    FakeWorker.instances.some(({ name }) => name === "noon-mixed-retained-engine"),
    false,
    "prepare must not select retained mode before authoring completes",
  );

  const prepareRequest = requestMessage(render, "prepare");
  assert.equal(prepareRequest.mode, undefined, "render preparation must remain mode-free");

  const startPromise = client.startRetainedCanonical(SCENE_SPEC_JSON);
  assert.equal(
    FakeWorker.instances.length,
    1,
    "engine startup must wait if authoring wins the race with render preparation",
  );

  acknowledgePreparation(render);
  await preparePromise;
  await new Promise((resolve) => setImmediate(resolve));

  const retainedEngine = workerByName(1, "noon-mixed-retained-engine");
  assert.equal(
    FakeWorker.instances.some(({ name }) => name === "noon-engine"),
    false,
    "prepared retained startup must never create a legacy engine",
  );

  const startRequest = requestMessage(render, "start_engine");
  assert.equal(startRequest.mode, "retained");
  assert.equal(startRequest.transportMode, "transferable");
  const engineInit = requestMessage(retainedEngine, "init");
  assert.equal(engineInit.sceneSpecJson, SCENE_SPEC_JSON);
  assert.equal("sceneJson" in engineInit, false);
  assert.equal("retainedDocumentJson" in engineInit, false);

  retainedEngine.emitMessage(
    engineMessage("ready", {
      transportMode: "transferable",
      canonical: true,
    }),
  );
  render.emitMessage(
    renderMessage("engine_started", {
      requestId: startRequest.requestId,
      mode: "retained",
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );

  const ready = await startPromise;
  assert.equal(ready.session, 1);
  assert.equal(ready.render.type, "engine_started");
  assert.equal(client.mode, "retained");
  client.terminate();
});

test("prepared render owner admits only one authored start while preparation is pending", async () => {
  resetWorkers();
  const client = new ExecutionWorkerClient(new FakeCanvas());

  const preparePromise = client.prepare({ transportMode: "transferable" });
  const render = workerByName(0, "noon-render");
  const retainedStart = client.startRetainedCanonical(SCENE_SPEC_JSON);
  let competingError = null;
  const competingStart = client.start(SCENE_JSON).catch((error) => {
    competingError = error;
  });

  acknowledgePreparation(render);
  await preparePromise;
  await new Promise((resolve) => setImmediate(resolve));

  const engineWorkers = FakeWorker.instances.filter(({ name }) => name.includes("engine"));
  assert.equal(
    engineWorkers.length,
    1,
    "one prepared render owner must never attach two competing engines",
  );
  assert.equal(engineWorkers[0].name, "noon-mixed-retained-engine");
  assert.match(competingError?.message ?? "", /already started/);
  await competingStart;

  const retainedEngine = engineWorkers[0];
  const startRequest = requestMessage(render, "start_engine");
  assert.equal(startRequest.mode, "retained");
  retainedEngine.emitMessage(
    engineMessage("ready", { transportMode: "transferable", canonical: true }),
  );
  render.emitMessage(
    renderMessage("engine_started", {
      requestId: startRequest.requestId,
      mode: "retained",
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );

  const ready = await retainedStart;
  assert.equal(ready.session, 1);
  assert.equal(client.mode, "retained");
  client.terminate();
});

test("failed prepared engine attachment replaces the transferred canvas and remains retryable", async () => {
  resetWorkers();
  const original = new FakeCanvas();
  const client = new ExecutionWorkerClient(original);

  const preparePromise = client.prepare({ transportMode: "transferable" });
  const render = workerByName(0, "noon-render");
  acknowledgePreparation(render);
  await preparePromise;

  const started = client.startRetainedCanonical(SCENE_SPEC_JSON);
  await new Promise((resolve) => setImmediate(resolve));
  const engine = workerByName(1, "noon-mixed-retained-engine");
  requestMessage(render, "start_engine");
  engine.emitError("prepared retained engine crashed");

  await assert.rejects(started, /prepared retained engine crashed/);
  assert.equal(engine.terminated, true);
  assert.equal(render.terminated, true);
  assert.equal(original.transferred, true);
  assert.notEqual(client.canvas, original);
  assert.equal(original.replacement, client.canvas);
  assert.equal(client.canvas.transferred, false);

  const retryOffset = FakeWorker.instances.length;
  const retry = client.startRetainedCanonical(SCENE_SPEC_JSON, {
    transportMode: "transferable",
  });
  const retryEngine = workerByName(retryOffset, "noon-mixed-retained-engine");
  const retryRender = workerByName(retryOffset, "noon-render");
  retryEngine.emitMessage(
    engineMessage("ready", { transportMode: "transferable", canonical: true }),
  );
  retryRender.emitMessage(
    renderMessage("ready", {
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );

  const ready = await retry;
  assert.equal(ready.session, 2, "failed prepared startup generation must not be reused");
  client.terminate();
});

test("prepare constructor failure restores the transferred canvas and remains retryable", async () => {
  resetWorkers();
  const original = new FakeCanvas();
  const client = new ExecutionWorkerClient(original);
  FakeWorker.failNextName = "noon-render";

  await assert.rejects(
    client.prepare({ transportMode: "transferable" }),
    /noon-render constructor failed/,
  );

  assert.equal(original.transferred, true);
  assert.notEqual(client.canvas, original);
  assert.equal(original.replacement, client.canvas);
  assert.equal(client.canvas.transferred, false);

  const retryOffset = FakeWorker.instances.length;
  const prepared = client.prepare({ transportMode: "transferable" });
  const render = workerByName(retryOffset, "noon-render");
  acknowledgePreparation(render);
  await prepared;
  client.terminate();
});

test("abandoning preparation restores the transferred canvas before any engine starts", async () => {
  resetWorkers();
  const original = new FakeCanvas();
  const client = new ExecutionWorkerClient(original);

  const prepared = client.prepare({ transportMode: "transferable" });
  const render = workerByName(0, "noon-render");
  acknowledgePreparation(render);
  await prepared;

  assert.equal(
    FakeWorker.instances.some(({ name }) => name.includes("engine")),
    false,
    "abandonment coverage must remain before engine selection",
  );
  client.terminate();

  assert.equal(render.terminated, true);
  assert.equal(original.transferred, true);
  assert.notEqual(client.canvas, original);
  assert.equal(original.replacement, client.canvas);
  assert.equal(client.canvas.transferred, false);

  const retryOffset = FakeWorker.instances.length;
  const retry = client.startRetainedCanonical(SCENE_SPEC_JSON, {
    transportMode: "transferable",
  });
  const retryEngine = workerByName(retryOffset, "noon-mixed-retained-engine");
  const retryRender = workerByName(retryOffset, "noon-render");
  retryEngine.emitMessage(
    engineMessage("ready", { transportMode: "transferable", canonical: true }),
  );
  retryRender.emitMessage(
    renderMessage("ready", {
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );

  const ready = await retry;
  assert.equal(ready.session, 1, "pre-engine preparation must not consume a session generation");
  client.terminate();
});
