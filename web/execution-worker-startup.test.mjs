import assert from "node:assert/strict";
import test from "node:test";

class FakeCanvas {
  clientWidth = 640;
  clientHeight = 360;
  width = 640;
  height = 360;
  className = "scene-canvas";
  id = "scene";
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
    clone.className = this.className;
    clone.id = this.id;
    return clone;
  }

  replaceWith(replacement) {
    this.replacement = replacement;
  }
}

class FakeMessageChannel {
  constructor() {
    this.port1 = {};
    this.port2 = {};
  }
}

class FakeWorker {
  static instances = [];
  static failNextName = null;

  listeners = new Map();
  messages = [];
  terminated = false;

  constructor(_url, options = {}) {
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
globalThis.MessageChannel = FakeMessageChannel;
globalThis.Worker = FakeWorker;
globalThis.window = { devicePixelRatio: 1 };

const { ExecutionWorkerClient } = await import("./execution-worker-client.js");

const SCENE_JSON = JSON.stringify({ version: 1, objects: [], tracks: [] });

function envelope(channel, type, payload = {}) {
  return { channel, protocolVersion: 1, type, ...payload };
}

function workerByName(offset, name) {
  const worker = FakeWorker.instances.slice(offset).find((candidate) => candidate.name === name);
  assert.ok(worker, `missing worker ${name}`);
  return worker;
}

function emitLegacyReady(offset) {
  const engine = workerByName(offset, "noon-engine");
  const render = workerByName(offset, "noon-render");
  engine.emitMessage(envelope("noon.engine", "ready", { transportMode: "transferable" }));
  render.emitMessage(
    envelope("noon.render", "ready", {
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );
  return { engine, render };
}

test("legacy constructor failure rolls back transferred canvas and retry succeeds", async () => {
  const original = new FakeCanvas();
  const client = new ExecutionWorkerClient(original);
  const offset = FakeWorker.instances.length;
  FakeWorker.failNextName = "noon-render";

  await assert.rejects(
    client.start(SCENE_JSON, { transportMode: "transferable" }),
    /noon-render constructor failed/,
  );

  const failedEngine = workerByName(offset, "noon-engine");
  assert.equal(failedEngine.terminated, true);
  assert.equal(original.transferred, true);
  assert.notEqual(client.canvas, original);
  assert.equal(original.replacement, client.canvas);
  assert.equal(client.canvas.transferred, false);

  const retryOffset = FakeWorker.instances.length;
  const retry = client.start(SCENE_JSON, { transportMode: "transferable" });
  emitLegacyReady(retryOffset);
  const ready = await retry;
  assert.equal(ready.session, 2, "failed startup generation must not be reused");
  client.terminate();
});
