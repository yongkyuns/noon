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

const { ExecutionWorkerClient } = await import("./execution-worker-client.js");

const SCENE_JSON = JSON.stringify({ version: 1, objects: [], tracks: [] });
const RETAINED_DOCUMENT_JSON = JSON.stringify({
  channel: "noon.authoring.retained",
  objects: [{ id: 1 }],
});

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
}

function rejected(promise) {
  return promise.then(
    () => null,
    (error) => error,
  );
}

test("stale prepared-start unwind cannot release a replacement generation reservation", async () => {
  FakeWorker.instances.length = 0;
  const client = new ExecutionWorkerClient(new FakeCanvas());

  const firstPrepare = client.prepare({ transportMode: "transferable" });
  const firstRender = workerByName(0, "noon-render");
  acknowledgePreparation(firstRender);
  await firstPrepare;

  const firstStartResult = rejected(
    client.startRetained(SCENE_JSON, RETAINED_DOCUMENT_JSON),
  );
  await new Promise((resolve) => setImmediate(resolve));
  assert.ok(
    FakeWorker.instances.some(({ name }) => name === "noon-mixed-retained-engine"),
    "first prepared start must reach engine attachment before cancellation",
  );

  client.terminate();

  const replacementOffset = FakeWorker.instances.length;
  const secondPrepareResult = rejected(client.prepare({ transportMode: "transferable" }));
  workerByName(replacementOffset, "noon-render");
  const secondStartResult = rejected(
    client.startRetained(SCENE_JSON, RETAINED_DOCUMENT_JSON),
  );

  const firstError = await firstStartResult;
  assert.match(firstError?.message ?? "", /terminated during an asynchronous operation/);

  await assert.rejects(
    client.start(SCENE_JSON),
    /already started/,
    "the old start's finally block must not clear the replacement start reservation",
  );

  client.terminate();
  const [secondPrepareError, secondStartError] = await Promise.all([
    secondPrepareResult,
    secondStartResult,
  ]);
  assert.match(secondPrepareError?.message ?? "", /terminated/);
  assert.match(secondStartError?.message ?? "", /terminated/);
});
