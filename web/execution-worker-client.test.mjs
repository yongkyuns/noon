import assert from "node:assert/strict";
import test from "node:test";

import {
  FakeCanvas, FakeMessageChannel, FakeWorker, FakeSemanticAuthoringClient,
} from "./test-support/execution-fakes.mjs";
globalThis.MessageChannel = FakeMessageChannel;

globalThis.HTMLCanvasElement = FakeCanvas;
globalThis.Worker = FakeWorker;
globalThis.window = { devicePixelRatio: 1 };

const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
const { resetRenderHostSelectionForTests } = await import("./render-host-selection.js");


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

function deferredRenderHostProbe(t) {
  const saved = new Map();
  let settleProbe;
  const probeReady = new Promise((resolve) => {
    settleProbe = resolve;
  });

  class ProbeWorker extends FakeWorker {
    #probe;

    constructor(url, options = {}) {
      super(url, options);
      this.#probe =
        String(url) === "blob:noon-render-host-probe#noon-render-capability-probe";
    }

    postMessage(message, transfer = []) {
      if (!this.#probe) {
        super.postMessage(message, transfer);
        return;
      }
      void probeReady.then((ok) => {
        this.onmessage?.({ data: { ok } });
      });
    }
  }

  for (const [name, value] of Object.entries({
    document: {
      createElement() {
        return new FakeCanvas();
      },
    },
    Worker: ProbeWorker,
    Blob: class {},
    URL: class extends URL {
      static createObjectURL() {
        return "blob:noon-render-host-probe";
      }

      static revokeObjectURL() {}
    },
    __NOON_RENDER_HOST__: null,
    location: { href: "https://example.test/" },
  })) {
    saved.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
    Object.defineProperty(globalThis, name, { configurable: true, writable: true, value });
  }
  resetRenderHostSelectionForTests();
  t.after(() => {
    resetRenderHostSelectionForTests();
    for (const [name, descriptor] of saved) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
  });
  return (ok = true) => settleProbe(ok);
}

async function finishStartup(starting, authoring, offset) {
  const render = await waitForNewWorker(offset, "noon-render");
  replyRender(render, await waitForRequest(render, "prepare"), "prepared");
  replyRender(render, await waitForRequest(render, "start_engine"), "engine_started");
  const ready = await starting;
  return { ready, render, engine: authoring.attachments.at(-1).controlPort.peer };
}

function replyRender(render, request, type) {
  render.emitMessage(renderMessage(type, {
    requestId: request.requestId, transportMode: "transferable", backend: "WebGL2",
  }));
}

async function startClient(errors = []) {
  const offset = FakeWorker.instances.length;
  const authoring = new FakeSemanticAuthoringClient();
  authoring.autoRespond = false;
  const client = new ExecutionWorkerClient(new FakeCanvas(), {
    onError(error, owner) { errors.push(`${owner}: ${error.message}`); },
  });
  const starting = client.startSemanticExecution("scene", authoring, { transportMode: "transferable" });
  const { ready, engine, render } = await finishStartup(starting, authoring, offset);
  assert.equal(ready.session, 1);
  return { client, authoring, engine, render };
}

function requestMessage(worker, type) {
  const entry = worker.messages.findLast(entry => (entry.message ?? entry).type === type);
  assert.ok(entry, `missing ${worker.name} ${type} request`);
  return entry.message ?? entry;
}

async function waitForRequest(worker, type, occurrence = 1) {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const entries = worker.messages.filter(entry => (entry.message ?? entry).type === type);
    if (entries.length >= occurrence) {
      return entries[occurrence - 1].message ?? entries[occurrence - 1];
    }
    await Promise.resolve();
  }
  assert.fail(`missing ${worker.name} ${type} request`);
}

test("reserves prepare and fresh startup while render-host probing is pending", async (t) => {
  const resolveProbe = deferredRenderHostProbe(t);
  const canvas = new FakeCanvas();
  const client = new ExecutionWorkerClient(canvas);
  const offset = FakeWorker.instances.length;

  const preparing = client.prepare({ transportMode: "transferable" });
  await assert.rejects(
    client.startSemanticExecution("scene", new FakeSemanticAuthoringClient(), { transportMode: "transferable" }),
    /already started/,
  );
  assert.equal(canvas.transfers, 0, "host probing must finish before canvas ownership transfers");

  resolveProbe();
  const render = await waitForNewWorker(offset, "noon-render");
  const prepareRequest = await waitForRequest(render, "prepare");
  render.emitMessage(
    renderMessage("prepared", {
      requestId: prepareRequest.requestId,
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );
  await preparing;
  assert.equal(canvas.transfers, 1);
  client.terminate();
});

test("reserves a fresh start and cancellation during host probing cannot transfer", async (t) => {
  const resolveProbe = deferredRenderHostProbe(t);
  const canvas = new FakeCanvas();
  const client = new ExecutionWorkerClient(canvas);

  const first = client.startSemanticExecution("scene", new FakeSemanticAuthoringClient(), { transportMode: "transferable" });
  await assert.rejects(
    client.startSemanticExecution("scene", new FakeSemanticAuthoringClient(), { transportMode: "transferable" }),
    /already started/,
  );
  client.terminate();
  resolveProbe();
  await assert.rejects(first, /terminated during an asynchronous operation/);
  assert.equal(canvas.transfers, 0);
});

test("engine and render requests use independent issuance spaces", async () => {
  const errors = [];
  const { client, engine, render } = await startClient(errors);

  const metricsPromise = client.metrics();
  await Promise.resolve();
  const engineMetrics = requestMessage(engine, "metrics");
  const renderMetrics = requestMessage(render, "metrics");
  assert.equal(engineMetrics.requestId, 0);
  assert.equal(renderMetrics.requestId, 2);

  engine.emitMessage(engineMessage("metrics", { requestId: 0, metrics: { host: {} } }));
  render.emitMessage(renderMessage("metrics", { requestId: 2, metrics: { ready: true } }));
  const metrics = await metricsPromise;
  assert.deepEqual(metrics.metrics, { ready: true });
  assert.deepEqual(metrics.engineMetrics, { host: {} });
  assert.equal(metrics.renderHost, "worker");

  assert.deepEqual(client.diagnostics, {
    session: 1,
    renderHost: "worker",
    engine: {
      nextRequestId: 1,
      pendingRequests: 0,
      staleResponses: 0,
      staleWorkerEvents: 0,
    },
    render: {
      nextRequestId: 3,
      pendingRequests: 0,
      staleResponses: 0,
      staleWorkerEvents: 0,
    },
  });
  assert.deepEqual(errors, []);
  client.terminate();
});

test("drops duplicate issued responses but keeps owner-local future IDs fatal", async () => {
  const errors = [];
  const { client, engine, render } = await startClient(errors);

  const statePromise = client.state();
  await Promise.resolve();
  const stateRequest = requestMessage(engine, "state");
  assert.equal(stateRequest.requestId, 0);
  const stateResponse = engineMessage("state", {
    requestId: 0,
    time: 0,
  });
  engine.emitMessage(stateResponse);
  await statePromise;

  engine.emitMessage(stateResponse);
  assert.equal(client.diagnostics.engine.staleResponses, 1);
  assert.deepEqual(errors, []);

  const metricsPromise = client.metrics();
  await Promise.resolve();
  const engineMetrics = requestMessage(engine, "metrics");
  const renderMetrics = requestMessage(render, "metrics");
  assert.equal(engineMetrics.requestId, 1);
  assert.equal(renderMetrics.requestId, 2);

  // Render has never issued request 4. A fatal protocol violation rejects the
  // current render request immediately instead of leaving it hung for restart.
  render.emitMessage(renderMessage("metrics", { requestId: 4, metrics: {} }));
  await assert.rejects(metricsPromise, /render worker returned unissued request ID 4/);
  assert.equal(client.diagnostics.render.pendingRequests, 0);
  assert.equal(client.diagnostics.render.staleResponses, 0);
  assert.equal(errors.length, 1);
  assert.match(errors[0], /render: render worker returned unissued request ID 4/);
  client.terminate();
});

test("ignores queued events from workers that were replaced by restart", async () => {
  const errors = [];
  const { client, authoring, engine: oldEngine, render: oldRender } = await startClient(errors);

  const beforeRestart = client.state();
  await Promise.resolve();
  const firstStateRequest = requestMessage(oldEngine, "state");
  assert.equal(firstStateRequest.requestId, 0);
  oldEngine.emitMessage(
    engineMessage("state", {
      requestId: 0,
      time: 0,
        }),
  );
  await beforeRestart;

  const offset = FakeWorker.instances.length;
  const restartPromise = client.restart();
  const { ready: restarted, engine: newEngine, render: newRender } =
    await finishStartup(restartPromise, authoring, offset);
  assert.equal(restarted.session, 2);
  assert.equal(oldEngine.closed, true);
  assert.equal(oldRender.terminated, true);

  oldEngine.emitMessage({ malformed: true });
  oldEngine.emitError("late engine crash");
  oldRender.emitMessageError();

  assert.deepEqual(errors, []);
  assert.equal(client.diagnostics.engine.staleWorkerEvents, 2);
  assert.equal(client.diagnostics.render.staleWorkerEvents, 1);

  const statePromise = client.state();
  await Promise.resolve();
  const stateRequest = requestMessage(newEngine, "state");
  assert.equal(
    stateRequest.requestId,
    1,
    "restart must not reuse an engine request ID that an old worker may still emit",
  );
  newEngine.emitMessage(
    engineMessage("state", {
      requestId: 1,
      time: 0,
        }),
  );
  await statePromise;
  assert.deepEqual(errors, []);
  client.terminate();
});

test("engine failure reconnects without replacing the render worker or canvas", async () => {
  const errors = [];
  const { client, authoring, engine: oldEngine, render } = await startClient(errors);
  const canvas = client.canvas;

  oldEngine.emitError("engine lost", {
    error: { stack: "Error: engine lost\n    at startRetained (engine-worker.js:42:7)" },
    filename: "engine-worker.js",
    lineno: 42,
    colno: 7,
  });
  assert.equal(errors.length, 1);
  assert.match(errors[0], /startRetained \(engine-worker\.js:42:7\)/);
  const initialError = errors[0];

  const offset = FakeWorker.instances.length;
  const restartPromise = client.restart();
  const attachRequest = await waitForRequest(render, "rebuild_engine");
  const newEngine = authoring.attachments.at(-1).controlPort.peer;
  assert.notEqual(newEngine, oldEngine);
  assert.equal(FakeWorker.instances.length, offset, "reconnect must reuse the existing render worker");
  replyRender(render, attachRequest, "engine_rebuilt");

  const restarted = await restartPromise;
  assert.equal(restarted.session, 2);
  assert.equal(client.canvas, canvas, "engine reconnect must preserve the transferred canvas");
  assert.equal(oldEngine.closed, true);
  assert.equal(render.terminated, false);

  oldEngine.emitMessage({ malformed: true });
  oldEngine.emitMessageError();
  assert.equal(client.diagnostics.engine.staleWorkerEvents, 2);
  assert.deepEqual(errors, [initialError]);

  const statePromise = client.state();
  await Promise.resolve();
  const stateRequest = requestMessage(newEngine, "state");
  newEngine.emitMessage(
    engineMessage("state", {
      requestId: stateRequest.requestId,
      time: 0,
        }),
  );
  await statePromise;
  client.terminate();
});

test("engine reconnect restores pause mode", async () => {
  const errors = [];
  const { client, authoring, engine: oldEngine, render } = await startClient(errors);
  const pausePromise = client.pause();
  const initialPause = await waitForRequest(oldEngine, "pause");
  oldEngine.emitMessage(
    engineMessage("result", {
      requestId: initialPause.requestId,
      operation: "pause",
      time: 0.5,
      playing: false,
        }),
  );
  await pausePromise;

  oldEngine.emitError("engine lost");
  const canvas = client.canvas;
  const offset = FakeWorker.instances.length;
  const restartPromise = client.restart();
  const renderAttach = await waitForRequest(render, "rebuild_engine");
  const attachment = authoring.attachments.at(-1);
  assert.equal(attachment.options.initiallyPaused, true, "shared attachment restores pause atomically");
  assert.equal(FakeWorker.instances.length, offset);
  replyRender(render, renderAttach, "engine_rebuilt");

  const restarted = await restartPromise;
  assert.equal(restarted.session, 2);
  assert.equal(client.canvas, canvas);
  assert.equal(render.terminated, false);
  assert.deepEqual(errors, ["engine: engine lost"]);
  client.terminate();
});

async function waitForNewWorker(offset, name) {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const worker = FakeWorker.instances.slice(offset).find((candidate) => candidate.name === name);
    if (worker !== undefined) return worker;
    await Promise.resolve();
  }
  assert.fail(`replacement ${name} worker must be created`);
}

test("render constructor failure rolls back transferred canvas and semantic startup can retry", async () => {
  const original = new FakeCanvas();
  const client = new ExecutionWorkerClient(original);
  const authoring = new FakeSemanticAuthoringClient();
  FakeWorker.failNextName = "noon-render";
  await assert.rejects(client.startSemanticExecution("scene", authoring, { transportMode: "transferable" }),
    /noon-render constructor failed/);
  assert.equal(original.transferred, true);
  assert.equal(original.replacement, client.canvas);
  assert.equal(client.canvas.transferred, false);
  assert.equal(authoring.attachments.length, 0, "failed render preparation must not attach a semantic context");
  const offset = FakeWorker.instances.length;
  const retry = client.startSemanticExecution("scene", authoring, { transportMode: "transferable" });
  const { ready } = await finishStartup(retry, authoring, offset);
  assert.equal(ready.session, 1, "no session was published by failed render preparation");
  client.terminate();
});
