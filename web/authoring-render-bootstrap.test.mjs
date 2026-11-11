import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

// Execute the real controller through its public host/port boundary. Replace
// only browser/GPU dependencies so the presentation/resize ordering is exact.
const controllerSource = await readFile(new URL("./authoring-render-controller.js", import.meta.url), "utf8");
const metricsSource = await readFile(new URL("./frame-metrics.js", import.meta.url), "utf8");
const executableSource = `${metricsSource}\n${controllerSource}`
  .replace(/^import\s+[\s\S]*?;\n/gm, "")
  .replace(/^export\s+/gm, "");
const flushTasks = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((accept, decline) => { resolve = accept; reject = decline; });
  return { promise, resolve, reject };
}

async function createHarness(backend, renderResults = [false, true]) {
  const frames = [], messages = [], gpu = deferred();
  let consume, renderCalls = 0, waits = 0, freed = false;
  class Canvas {
    width = 1;
    height = 1;
    addEventListener() {}
    removeEventListener() {}
  }
  class Port {
    messages = [];
    addEventListener(type, callback) { this.onmessage = callback; }
    start() {}
    close() {}
    postMessage(message) { this.messages.push(message); }
  }
  const port = new Port();
  const renderer = {
    applyDeltaTransportJson: () => true,
    resize() {},
    render: () => { renderCalls += 1; return renderResults.shift() ?? true; },
    rendererBackend: () => backend,
    waitForSubmittedWork: () => { waits += 1; return gpu.promise; },
    gpuGeneration: () => 1,
    time: () => 0,
    objectCount: () => 1,
    preloadedGeometryCount: () => 1,
    preloadBytesUploaded: () => 4,
    free: () => { freed = true; },
  };
  const context = vm.createContext({
    performance, Promise, Uint8Array, setTimeout, clearTimeout,
    OffscreenCanvas: Canvas, MessagePort: Port,
    init: async () => {},
    RetainedExecutionCanvasRenderer: { create: async () => renderer },
    TransferableExecutionDeltaReceiver: class {
      constructor(_port, callback) { consume = callback; }
      drain() {}
      pendingCount() { return 0; }
    },
    EXECUTION_TRANSPORT_SHARED: "shared",
    EXECUTION_TRANSPORT_TRANSFERABLE: "transferable",
    drainRendererGpuDiagnostics: () => true,
    formatGpuDiagnostic: String,
    host: { postMessage: message => messages.push(message),
      requestAnimationFrame: callback => { frames.push(callback); return frames.length; } },
  });
  vm.runInContext(executableSource, context);
  const controller = vm.runInContext("createAuthoringRenderController(host)", context);
  const dispatch = message => controller.dispatch({ channel: "noon.render", protocolVersion: 1, ...message });
  await dispatch({ type: "init", canvas: new Canvas(), port, mode: "retained", transportMode: "transferable" });
  port.onmessage({ data: { type: "retained_resources", bytes: new Uint8Array([1]) } });
  const publication = { session: 11, sequence: 0 };
  assert.equal(consume(null, publication, "initial"), true);
  await flushTasks();
  const ready = () => messages.filter(message => message.type === "ready");
  const receipts = () => port.messages.filter(message => message.type === "execution_presented");
  return { frames, messages, gpu, publication, dispatch, ready, receipts,
    renderCalls: () => renderCalls, waits: () => waits, freed: () => freed,
    async dispose() {
      controller.shutdown();
      gpu.resolve();
      for (const callback of frames.splice(0)) callback(40);
      await flushTasks();
    },
  };
}

for (const backend of ["WebGPU", "WebGL2"]) {
  test(`${backend}: resize presentation releases the pending bootstrap retry`, async () => {
    const h = await createHarness(backend);
    try {
      assert.equal(h.renderCalls(), 1);
      assert.equal(h.ready().length, 0, "failed first render cannot report ready");
      assert.equal(h.frames.length, 1);
      await h.dispatch({ type: "resize", width: 2, height: 2 });
      assert.equal(h.renderCalls(), 2, "resize presents the waiting snapshot");
      assert.equal(h.ready().length, 0);
      assert.equal(h.receipts().length, 0, "startup retains source-admission ownership");
      h.frames.shift()(20);
      await flushTasks();
      assert.equal(h.waits(), backend === "WebGPU" ? 1 : 0);
      if (backend === "WebGPU") {
        assert.equal(h.ready().length, 0, "cold GPU completion still gates readiness");
        assert.equal(h.receipts().length, 0);
        h.gpu.resolve();
        await flushTasks();
      }
      assert.equal(h.renderCalls(), 2, "clean retained content does not need another presentation");
      assert.equal(h.ready().length, 1, "resize must not strand the startup promise");
      assert.equal(h.ready()[0].presentedFrames, 1);
      assert.deepEqual(h.receipts().map(({ session, sequence }) => ({ session, sequence })), [h.publication]);
      assert.equal(h.frames.length, 1, "exactly one current render loop starts");
      assert.equal(h.messages.some(message => message.type === "error"), false);
    } finally { await h.dispose(); }
  });

  test(`${backend}: unsuccessful resize cannot bypass the first presentation`, async () => {
    const h = await createHarness(backend, [false, false, true]);
    try {
      await h.dispatch({ type: "resize", width: 2, height: 2 });
      assert.equal(h.renderCalls(), 2);
      assert.equal(h.ready().length, 0);
      assert.equal(h.receipts().length, 0);
      assert.equal(h.waits(), 0, "there is no submitted frame to fence yet");
      h.frames.shift()(20);
      await flushTasks();
      h.gpu.resolve();
      await flushTasks();
      assert.equal(h.renderCalls(), 3);
      assert.equal(h.ready().length, 1);
      assert.equal(h.ready()[0].presentedFrames, 1);
      assert.equal(h.receipts().length, 1);
    } finally { await h.dispose(); }
  });

  test(`${backend}: stop after resize presentation cannot revive bootstrap`, async () => {
    const h = await createHarness(backend);
    await h.dispatch({ type: "resize", width: 2, height: 2 });
    await h.dispose();
    assert.equal(h.freed(), true);
    assert.equal(h.ready().length, 0);
    assert.equal(h.receipts().length, 0);
    assert.equal(h.frames.length, 0);
  });
}

test("WebGPU: failed cold work after resize presentation cannot admit the source", async () => {
  const h = await createHarness("WebGPU");
  try {
    await h.dispatch({ type: "resize", width: 2, height: 2 });
    h.frames.shift()(20);
    await flushTasks();
    assert.equal(h.waits(), 1);
    h.gpu.reject(new Error("cold GPU work failed"));
    await flushTasks();
    assert.equal(h.ready().length, 0);
    assert.equal(h.receipts().length, 0);
    assert.equal(h.frames.length, 0);
    assert.match(h.messages.find(message => message.type === "error")?.message, /cold GPU work failed/);
  } finally { await h.dispose(); }
});
