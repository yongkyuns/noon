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

  listeners = new Map();
  messages = [];
  terminated = false;

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

const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");

const SCENE_SPEC_JSON = JSON.stringify({
  version: 1,
  objects: [{ id: 1 }],
  tracks: [],
});
const NON_DEFAULT_SHARED_SLOT_CAPACITY = 2 * 1024 * 1024;

function envelope(channel, type, payload = {}) {
  return { channel, protocolVersion: 1, type, ...payload };
}

function workerByName(offset, name) {
  const worker = FakeWorker.instances.slice(offset).find((candidate) => candidate.name === name);
  assert.ok(worker, `missing worker ${name}`);
  return worker;
}

function requestMessage(worker, type) {
  const entry = worker.messages.findLast(({ message }) => message.type === type);
  assert.ok(entry, `missing ${worker.name} ${type} request`);
  return entry.message;
}

function acknowledgePreparation(render) {
  const request = requestMessage(render, "prepare");
  render.emitMessage(
    envelope("noon.render", "prepared", {
      requestId: request.requestId,
      transportMode: "transferable",
      width: 640,
      height: 360,
    }),
  );
  return request;
}

function finishRetainedStart(render, engine) {
  const startRequest = requestMessage(render, "start_engine");
  engine.emitMessage(
    envelope("noon.engine", "ready", {
      transportMode: "transferable",
      retained: true,
      mixed: true,
    }),
  );
  render.emitMessage(
    envelope("noon.render", "engine_started", {
      requestId: startRequest.requestId,
      mode: "retained",
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );
}

test("prepared authoring execution stays unpublished until the canonical retained engine is ready", async () => {
  FakeWorker.instances.length = 0;
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const preparation = client.prepare({ transportMode: "transferable" });
  const render = workerByName(0, "noon-render");
  const prepareRequest = requestMessage(render, "prepare");

  assert.equal(client.mode, null, "mode must remain unpublished during render preparation");
  await assert.rejects(
    client.state(),
    /AuthoringExecutionClient has not been started/,
    "prepared resources must not make public execution state available",
  );

  const started = client.startRetainedCanonical(SCENE_SPEC_JSON);
  assert.equal(client.mode, null, "mode must remain unpublished while preparation is pending");
  assert.equal(
    FakeWorker.instances.some(({ name }) => name === "noon-mixed-retained-engine"),
    false,
    "canonical authored engine startup must wait for the render preparation barrier",
  );

  render.emitMessage(
    envelope("noon.render", "prepared", {
      requestId: prepareRequest.requestId,
      transportMode: "transferable",
      width: 640,
      height: 360,
    }),
  );
  await preparation;
  await new Promise((resolve) => setImmediate(resolve));

  const engine = workerByName(1, "noon-mixed-retained-engine");
  finishRetainedStart(render, engine);

  const ready = await started;
  assert.equal(ready.session, 1);
  assert.equal(client.mode, "retained");
  assert.equal(client.rendererBackend, "WebGL2");
  client.terminate();
});

test("prepared canonical startup inherits a non-default shared slot capacity", async () => {
  FakeWorker.instances.length = 0;
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const preparation = client.prepare({
    transportMode: "transferable",
    sharedSlotCapacity: NON_DEFAULT_SHARED_SLOT_CAPACITY,
  });
  const render = workerByName(0, "noon-render");
  acknowledgePreparation(render);
  await preparation;

  const started = client.startRetainedCanonical(SCENE_SPEC_JSON);
  await new Promise((resolve) => setImmediate(resolve));
  const engine = workerByName(1, "noon-mixed-retained-engine");
  const init = requestMessage(engine, "init");
  assert.equal(
    init.sharedSlotCapacity,
    NON_DEFAULT_SHARED_SLOT_CAPACITY,
    "canonical startup must inherit prepared transport capacity without an explicit override",
  );
  finishRetainedStart(render, engine);
  await started;
  client.terminate();
});

test("terminating during render preparation cancels the unpublished candidate and adopts its fresh canvas", async () => {
  FakeWorker.instances.length = 0;
  const original = new FakeCanvas();
  const client = new AuthoringExecutionClient(original);
  const preparation = client.prepare({ transportMode: "transferable" });
  const render = workerByName(0, "noon-render");

  client.terminate();

  await assert.rejects(
    preparation,
    /AuthoringExecutionClient was terminated during an asynchronous operation/,
  );
  assert.equal(render.terminated, true);
  assert.equal(original.transferred, true);
  assert.notEqual(client.canvas, original);
  assert.equal(original.replacement, client.canvas);
  assert.equal(client.canvas.transferred, false);
  assert.equal(client.mode, null);
  await assert.rejects(client.state(), /AuthoringExecutionClient has not been started/);
});
