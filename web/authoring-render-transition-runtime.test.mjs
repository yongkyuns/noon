import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const workerSource = await readFile(new URL("./authoring-render-worker.js", import.meta.url), "utf8");
const executableSource = workerSource.replace(/^import\s+[\s\S]*?;\n/gm, "");

function deferred() {
  let resolve;
  const promise = new Promise((accept) => {
    resolve = accept;
  });
  return { promise, resolve };
}

function flushTasks() {
  return new Promise((resolve) => setImmediate(resolve));
}

class FakePort {
  constructor() {
    this.messages = [];
    this.closed = false;
  }

  addEventListener() {}
  start() {}
  close() { this.closed = true; }
  postMessage(message) { this.messages.push(message); }
}

function createRenderer(renderResults) {
  return {
    freed: false,
    applyDeltaJson: () => true,
    resize() {},
    render: () => renderResults.shift() ?? true,
    rendererBackend: () => "WebGPU",
    gpuGeneration: () => 1,
    time: () => 0,
    preloadedGeometryCount: () => 1200,
    preloadBytesUploaded: () => 1024,
    free() { this.freed = true; },
  };
}

function createWorkerHarness(renderResults = [false, true]) {
  const creation = deferred();
  const animationFrames = [];
  const mainMessages = [];
  const context = vm.createContext({
    console,
    performance,
    Promise,
    Uint8Array,
    setTimeout,
    clearTimeout,
    OffscreenCanvas: class {},
    MessagePort: FakePort,
    ExecutionCanvasRenderer: { create: async () => createRenderer([true]) },
    RetainedExecutionCanvasRenderer: { create: () => creation.promise },
    SharedExecutionDeltaReader: class { drain() { return 0; } },
    TransferableExecutionDeltaReceiver: class { drain() {} },
    EXECUTION_TRANSPORT_SHARED: "shared",
    EXECUTION_TRANSPORT_TRANSFERABLE: "transferable",
    drainRendererGpuDiagnostics: () => true,
    formatGpuDiagnostic: String,
    self: {
      addEventListener() {},
      close() {},
      postMessage(message) { mainMessages.push(message); },
      requestAnimationFrame(callback) { animationFrames.push(callback); },
    },
  });
  vm.runInContext(executableSource, context);
  const oldPort = new FakePort();
  const nextPort = new FakePort();
  const oldRenderer = createRenderer([true]);
  context.oldPort = oldPort;
  context.nextPort = nextPort;
  context.oldRenderer = oldRenderer;
  vm.runInContext(
    `
canvas = {};
transportMode = EXECUTION_TRANSPORT_TRANSFERABLE;
mode = MODE_LEGACY;
renderer = oldRenderer;
renderPort = oldPort;
running = true;
scheduleFrame();
beginRendererTransition(
  { port: nextPort, transportMode, requestId: 7 },
  MODE_RETAINED,
  "mode_switched",
);
handleRetainedResources({ bytes: new Uint8Array([1]) });
consumeDelta("initial");
`,
    context,
  );
  const createdRenderer = createRenderer([...renderResults]);
  return { context, creation, animationFrames, mainMessages, nextPort, oldRenderer, createdRenderer };
}

test("delayed retained transition gates stale ticks and retries presentation before ready", async () => {
  const harness = createWorkerHarness();
  assert.equal(harness.animationFrames.length, 1, "the old loop has one queued callback");
  assert.equal(harness.nextPort.messages.length, 0);

  harness.creation.resolve(harness.createdRenderer);
  await flushTasks();
  assert.equal(harness.animationFrames.length, 2, "failed presentation queues a retry only");

  harness.animationFrames.shift()(10);
  assert.equal(harness.nextPort.messages.length, 0, "stale loop callback must not tick candidate");
  assert.equal(harness.animationFrames.length, 1, "stale callback must not reschedule itself");

  harness.animationFrames.shift()(20);
  await flushTasks();
  const ready = harness.mainMessages.filter((message) => message.type === "mode_switched");
  assert.equal(ready.length, 1, "ready publishes once after successful presentation");
  assert.equal(ready[0].time, 0);
  assert.equal(ready[0].presentedFrames, 1);
  assert.equal(harness.nextPort.messages.length, 0, "candidate receives no tick before ready");
  assert.equal(harness.animationFrames.length, 1, "ready schedules exactly one current loop");

  harness.animationFrames.shift()(30);
  assert.equal(
    harness.nextPort.messages.filter((message) => message.type === "tick").length,
    1,
  );
  assert.equal(harness.animationFrames.length, 1, "current loop reschedules exactly once");
});

test("stop during presentation retry disposes renderer without ready or tick", async () => {
  const harness = createWorkerHarness([false]);
  harness.creation.resolve(harness.createdRenderer);
  await flushTasks();
  assert.equal(harness.animationFrames.length, 2);

  vm.runInContext("stop();", harness.context);
  harness.animationFrames.shift()(10);
  harness.animationFrames.shift()(20);
  await flushTasks();

  assert.equal(harness.createdRenderer.freed, true);
  assert.equal(harness.mainMessages.some((message) => message.type === "mode_switched"), false);
  assert.equal(harness.nextPort.messages.some((message) => message.type === "tick"), false);
  assert.equal(harness.animationFrames.length, 0);
});
