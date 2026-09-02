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
const SCENE_SPEC_JSON = JSON.stringify({ version: 1, objects: [], tracks: [] });

function workerByName(offset, name) {
  const worker = FakeWorker.instances.slice(offset).find((candidate) => candidate.name === name);
  assert.ok(worker, `missing worker ${name}`);
  return worker;
}

function rejected(promise) {
  return promise.then(
    () => null,
    (error) => error,
  );
}

test("stale prepared-start unwind cannot release a replacement generation reservation", async () => {
  FakeWorker.instances.length = 0;
  const originalCanvas = new FakeCanvas();
  const client = new ExecutionWorkerClient(originalCanvas);

  // Keep the first render preparation pending. Starting retained execution now
  // installs a prepared-start reservation and waits on that preparation without
  // attaching an engine yet.
  const firstPrepareResult = rejected(client.prepare({ transportMode: "transferable" }));
  workerByName(0, "noon-render");
  const firstStartResult = rejected(client.startRetainedCanonical(SCENE_SPEC_JSON));

  // Terminating a prepare-only generation restores a fresh HTML canvas. This is
  // the current reusable lifecycle boundary; once an engine has attached, the
  // transferred low-level client itself is intentionally not reused.
  client.terminate();
  assert.notEqual(client.canvas, originalCanvas, "prepare cancellation must restore a fresh canvas");
  assert.equal(originalCanvas.replacement, client.canvas);

  // Install the replacement generation synchronously before awaiting the stale
  // promises. Their queued unwind must not clear this newer reservation.
  const replacementOffset = FakeWorker.instances.length;
  const secondPrepareResult = rejected(client.prepare({ transportMode: "transferable" }));
  workerByName(replacementOffset, "noon-render");
  const secondStartResult = rejected(client.startRetainedCanonical(SCENE_SPEC_JSON));

  const [firstPrepareError, firstStartError] = await Promise.all([
    firstPrepareResult,
    firstStartResult,
  ]);
  assert.match(firstPrepareError?.message ?? "", /terminated/);
  assert.match(firstStartError?.message ?? "", /terminated during an asynchronous operation/);

  await assert.rejects(
    client.start(SCENE_JSON),
    /already started/,
    "the stale start's finally block must not clear the replacement start reservation",
  );

  client.terminate();
  const [secondPrepareError, secondStartError] = await Promise.all([
    secondPrepareResult,
    secondStartResult,
  ]);
  assert.match(secondPrepareError?.message ?? "", /terminated/);
  assert.match(secondStartError?.message ?? "", /terminated/);
});
