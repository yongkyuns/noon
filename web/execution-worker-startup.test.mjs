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

const { AuthoringExecutionClient, AUTHORING_EXECUTION_LEGACY } =
  await import("./authoring-execution-client.js");
const { ExecutionWorkerClient } = await import("./execution-worker-client.js");

const SCENE_JSON = JSON.stringify({ version: 1, objects: [], tracks: [] });
const SCENE_SPEC_JSON = JSON.stringify({ version: 1, objects: [], tracks: [] });

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

function emitUnifiedRetainedReady(offset) {
  const engine = workerByName(offset, "noon-mixed-retained-engine");
  const render = workerByName(offset, "noon-render");
  engine.emitMessage(
    envelope("noon.engine", "ready", {
      transportMode: "transferable",
      retained: true,
      mixed: true,
      canonical: true,
    }),
  );
  render.emitMessage(
    envelope("noon.render", "ready", {
      transportMode: "transferable",
      backend: "WebGL2",
      retained: true,
      mixed: true,
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

test("canonical retained startup failure rolls back transferred canvas and retry succeeds", async () => {
  const original = new FakeCanvas();
  const errors = [];
  const client = new ExecutionWorkerClient(original, {
    onError(error, owner) {
      errors.push(`${owner}: ${error.message}`);
    },
  });
  const offset = FakeWorker.instances.length;
  const started = client.startRetainedCanonical(SCENE_SPEC_JSON, {
    transportMode: "transferable",
  });
  const engine = workerByName(offset, "noon-mixed-retained-engine");
  const render = workerByName(offset, "noon-render");
  engine.emitMessage(
    envelope("noon.engine", "ready", {
      transportMode: "transferable",
      retained: true,
      mixed: true,
      canonical: true,
    }),
  );
  render.emitError("canonical retained render startup crashed");

  await assert.rejects(started, /canonical retained render startup crashed/);
  assert.equal(engine.terminated, true);
  assert.equal(render.terminated, true);
  assert.equal(original.transferred, true);
  assert.notEqual(client.canvas, original);
  assert.equal(original.replacement, client.canvas);
  assert.equal(client.canvas.transferred, false);
  assert.deepEqual(errors, ["render: canonical retained render startup crashed"]);

  const retryOffset = FakeWorker.instances.length;
  const retry = client.startRetainedCanonical(SCENE_SPEC_JSON, {
    transportMode: "transferable",
  });
  emitUnifiedRetainedReady(retryOffset);
  const ready = await retry;
  assert.equal(ready.session, 2, "failed retained startup generation must not be reused");
  client.terminate();
});

test("authoring router adopts a low-level replacement after failed initial start", async () => {
  const original = new FakeCanvas();
  const router = new AuthoringExecutionClient(original);
  FakeWorker.failNextName = "noon-render";

  await assert.rejects(
    router.start(SCENE_JSON, { transportMode: "transferable" }),
    /noon-render constructor failed/,
  );
  assert.notEqual(router.canvas, original);
  assert.equal(original.replacement, router.canvas);

  const retryOffset = FakeWorker.instances.length;
  const retry = router.start(SCENE_JSON, { transportMode: "transferable" });
  emitLegacyReady(retryOffset);
  await retry;
  assert.equal(router.mode, AUTHORING_EXECUTION_LEGACY);
  router.terminate();
});

test("authoring restart remains retryable after transient startup failure", async () => {
  const router = new AuthoringExecutionClient(new FakeCanvas());
  const initialOffset = FakeWorker.instances.length;
  const initial = router.start(SCENE_JSON, { transportMode: "transferable" });
  emitLegacyReady(initialOffset);
  await initial;

  const beforeFailure = router.canvas;
  FakeWorker.failNextName = "noon-render";
  await assert.rejects(router.restart(), /noon-render constructor failed/);
  assert.notEqual(router.canvas, beforeFailure);
  assert.equal(router.canvas.transferred, false);

  const retryOffset = FakeWorker.instances.length;
  const retry = router.restart();
  emitLegacyReady(retryOffset);
  const ready = await retry;
  assert.equal(ready.session, 3, "restart retry must advance past the failed generation");
  assert.equal(router.mode, AUTHORING_EXECUTION_LEGACY);
  router.terminate();
});
