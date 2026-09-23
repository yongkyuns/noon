import assert from "node:assert/strict";
import test from "node:test";

import {
  FakeCanvas, FakeMessageChannel, FakeWorker, FakeSemanticAuthoringClient,
} from "./test-support/execution-fakes.mjs";
globalThis.MessageChannel = FakeMessageChannel;

globalThis.HTMLCanvasElement = FakeCanvas;
globalThis.Worker = FakeWorker;
globalThis.window = { devicePixelRatio: 1 };

const { ExecutionWorkerClient, MAX_IN_FLIGHT_NATIVE_INPUTS } = await import("./execution-worker-client.js");
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


const observeResult = (promise) => promise.then(
  value => ({ value }), error => ({ error }),
);

function nativeInputCall(client, index) {
  switch (index % 3) {
    case 0: return client.setNativeStateInput(
      { kind: "control", name: "gain" }, { kind: "scalar", value: index },
    );
    case 1: return client.emitNativeEvent({ kind: "control_commit", name: `edge-${index}` });
    default: return client.submitBrowserPointerInput({
      kind: "move", surface_x: index, surface_y: 0,
      viewport_width: 800, viewport_height: 400, view_revision: 1,
    });
  }
}

test("native input bounds reservations before readiness and leaves control requests available", async () => {
  const { client, engine } = await startClient();
  const count = MAX_IN_FLIGHT_NATIVE_INPUTS;
  const results = [];
  try {
    // Same-turn submission exercises reservations before the first ready() await.
    for (let i = 0; i < count; i += 1) results.push(observeResult(nativeInputCall(client, i)));
    const excess = observeResult(nativeInputCall(client, count));
    await new Promise(resolve => setImmediate(resolve));
    const sent = engine.messages.filter(message => /^(native_|browser_pointer)/.test(message.type));
    assert.equal(sent.length, count, "overflow must not allocate a request or post a message");
    assert.match((await excess).error?.message ?? "", /native input.*full/i);
    assert.equal(client.diagnostics.engine.nextRequestId, count);
    assert.equal(client.diagnostics.engine.pendingRequests, count);

    const state = client.state();
    const stateRequest = await waitForRequest(engine, "state");
    engine.emitMessage(engineMessage("state", { requestId: stateRequest.requestId, time: 0 }));
    await state;
    engine.emitMessage(engineMessage(sent[0].type, { requestId: sent[0].requestId, time: 0 }));
    assert.ok((await results[0]).value);
    results.push(observeResult(nativeInputCall(client, count + 1)));
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(client.diagnostics.engine.pendingRequests, count);
  } finally {
    client.terminate();
    await Promise.all(results);
  }
  assert.equal(client.diagnostics.engine.pendingRequests, 0);
});

test("native input snapshots occurrence data before its readiness await", async () => {
  const { client, engine } = await startClient();
  const input = { kind: "press", surface_x: 10, surface_y: 20, button: 0, view_revision: 3 };
  const expected = { ...input };
  const outcome = observeResult(client.submitBrowserPointerInput(input));
  input.surface_x = 90;
  input.kind = "release";
  input.button = 2;
  try {
    const sent = await waitForRequest(engine, "browser_pointer_input");
    const { channel, protocolVersion, requestId, type, ...body } = sent;
    assert.deepEqual(body, { input: expected, presentation: null });
    engine.emitMessage(engineMessage(type, { requestId, time: 0 }));
    assert.ok((await outcome).value);
  } finally { client.terminate(); await outcome; }
});

test("native input transport metadata cannot override its issued request identity", async () => {
  const { client, engine } = await startClient();
  const outcome = observeResult(client.submitBrowserPointerInput({
    kind: "move", surface_x: 1, surface_y: 2,
    channel: "wrong", protocolVersion: 77, requestId: 987, type: "seek", time: 3,
  }));
  try {
    await new Promise(resolve => setImmediate(resolve));
    const sent = engine.messages.at(-1);
    assert.equal(sent.channel, "noon.engine");
    assert.equal(sent.protocolVersion, 1);
    assert.equal(sent.type, "browser_pointer_input");
    assert.equal(sent.requestId, 0);
    engine.emitMessage(engineMessage(sent.type, { requestId: 0 }));
    assert.ok((await outcome).value);
  } finally { client.terminate(); await outcome; }
});

test("native input releases capacity and pending requests after synchronous post failures", async () => {
  const { client, engine } = await startClient();
  const post = engine.postMessage.bind(engine);
  engine.postMessage = message => {
    if (message.type === "native_event") throw new Error("cannot clone input");
    post(message);
  };
  try {
    for (let i = 0; i < MAX_IN_FLIGHT_NATIVE_INPUTS * 2; i += 1) {
      await assert.rejects(client.emitNativeEvent({ kind: "wheel" }), /cannot clone input/);
      assert.equal(client.diagnostics.engine.pendingRequests, 0);
    }
  } finally { client.terminate(); }
});

test("failed pointer view reservation stays unregistered and the same revision can retry", async () => {
  const { client, engine } = await startClient();
  const held = [];
  try {
    for (let i = 0; i < MAX_IN_FLIGHT_NATIVE_INPUTS; i += 1) {
      held.push(observeResult(client.emitNativeEvent({ kind: "control_commit", name: `held-${i}` })));
    }
    await new Promise(resolve => setImmediate(resolve));
    const failed = observeResult(client.setBrowserPointerView(9, 800, 400));
    assert.match((await failed).error?.message ?? "", /native input.*full/i);
    assert.equal(client.pointerPresentation, null);

    const first = await waitForRequest(engine, "native_event", 1);
    engine.emitMessage(engineMessage(first.type, { requestId: first.requestId }));
    await held[0];

    const retry = client.setBrowserPointerView(9, 800, 400);
    const registration = await waitForRequest(engine, "browser_pointer_view", 1);
    assert.deepEqual(registration.view, { revision: 9, width: 800, height: 400 });
    engine.emitMessage(engineMessage(registration.type, { requestId: registration.requestId }));
    await retry;
  } finally {
    client.terminate();
    await Promise.all(held);
  }
});

test("duplicate pointer view registration shares the pending bounded delivery", async () => {
  const { client, engine } = await startClient();
  try {
    const first = client.setBrowserPointerView(11, 800, 400);
    const duplicate = client.setBrowserPointerView(11, 800, 400);
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(engine.messages.filter(message => message.type === "browser_pointer_view").length, 1);
    const registration = await waitForRequest(engine, "browser_pointer_view", 1);
    engine.emitMessage(engineMessage(registration.type, { requestId: registration.requestId }));
    await Promise.all([first, duplicate]);
  } finally { client.terminate(); }
});

test("native input cannot cross a scene switch during its readiness await", async () => {
  const { client, engine: oldEngine, render } = await startClient();
  const input = observeResult(client.submitBrowserPointerInput({ kind: "cancel", view_revision: 1 }));
  const replacement = new FakeSemanticAuthoringClient();
  const switching = client.switchToSemanticExecution("replacement", replacement);
  try {
    const rebuild = await waitForRequest(render, "rebuild_engine");
    replyRender(render, rebuild, "engine_rebuilt");
    await switching;
    assert.match((await input).error?.message ?? "", /retired|transition/i);
    const newEngine = replacement.attachments.at(-1).controlPort.peer;
    assert.equal(oldEngine.messages.some(message => message.type === "browser_pointer_input"), false);
    assert.equal(newEngine.messages.some(message => message.type === "browser_pointer_input"), false);
  } finally { client.terminate(); await input; }
});

test("native input releases every reservation after remote rejection and termination", async () => {
  const { client, engine, authoring } = await startClient();
  const count = MAX_IN_FLIGHT_NATIVE_INPUTS;
  let outcomes = Array.from({ length: count }, (_, i) => observeResult(nativeInputCall(client, i)));
  try {
    await new Promise(resolve => setImmediate(resolve));
    for (const message of engine.messages) {
      engine.emitMessage(engineMessage("error", { requestId: message.requestId, message: "input rejected" }));
    }
    assert.ok((await Promise.all(outcomes)).every(result => result.error));
    assert.equal(client.diagnostics.engine.pendingRequests, 0);
    outcomes = Array.from({ length: count }, (_, i) => observeResult(nativeInputCall(client, i)));
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(client.diagnostics.engine.pendingRequests, count);
    client.terminate({ preserveHostConfiguration: true });
    assert.ok((await Promise.all(outcomes)).every(result => result.error));
    const offset = FakeWorker.instances.length;
    const restarting = client.restart();
    const { engine: restarted } = await finishStartup(restarting, authoring, offset);
    const input = observeResult(client.emitNativeEvent({ kind: "wheel" }));
    const sent = await waitForRequest(restarted, "native_event");
    restarted.emitMessage(engineMessage(sent.type, { requestId: sent.requestId }));
    assert.ok((await input).value);
  } finally { client.terminate(); await Promise.all(outcomes); }
});


test("uncloneable native input releases its reservation without allocating a transport ID", async () => {
  const { client, engine } = await startClient();
  try {
    for (let i = 0; i < MAX_IN_FLIGHT_NATIVE_INPUTS * 2; i += 1) {
      await assert.rejects(client.submitBrowserPointerInput({ kind: "move", invalid: () => {} }));
    }
    assert.equal(client.diagnostics.engine.nextRequestId, 0);
    assert.equal(client.diagnostics.engine.pendingRequests, 0);
    assert.equal(engine.messages.length, 0);
    const accepted = observeResult(client.emitNativeEvent({ kind: "wheel" }));
    const message = await waitForRequest(engine, "native_event");
    engine.emitMessage(engineMessage(message.type, { requestId: message.requestId }));
    assert.ok((await accepted).value);
  } finally { client.terminate(); }
});

test("selection policy uses the bounded input lane and validates before reservation", async () => {
  const { client, engine } = await startClient();
  const pending = [];
  try {
    for (const invalid of [undefined, -1, NaN, Infinity, "4", {}, true]) {
      await assert.rejects(client.setPointerFillSelection(invalid), /selection tolerance/);
    }
    assert.equal(client.diagnostics.engine.pendingRequests, 0);
    assert.equal(client.diagnostics.engine.nextRequestId, 0);
    pending.push(observeResult(client.setPointerFillSelection(4)));
    for (let i = 1; i < MAX_IN_FLIGHT_NATIVE_INPUTS; i += 1) {
      pending.push(observeResult(nativeInputCall(client, i)));
    }
    await assert.rejects(client.setPointerFillSelection(null), /native input.*full/i);
    const configuration = await waitForRequest(engine, "pointer_fill_selection");
    assert.equal(configuration.maxMovement, 4);
    assert.equal(client.diagnostics.engine.pendingRequests, MAX_IN_FLIGHT_NATIVE_INPUTS);
    engine.emitMessage(engineMessage(configuration.type, { requestId: configuration.requestId, time: 0 }));
    assert.equal((await pending[0]).value.time, 0);
    pending.push(observeResult(client.setPointerFillSelection(null)));
    await new Promise(resolve => setImmediate(resolve));
    const clear = engine.messages.filter(m => m.type === "pointer_fill_selection").at(-1);
    assert.equal(clear.maxMovement, null);
    engine.emitMessage(engineMessage(clear.type, { requestId: clear.requestId, time: 0 }));
    assert.equal((await pending.at(-1)).value.time, 0);
  } finally { client.terminate(); await Promise.all(pending); }
});

test("selection configuration is not replayed through a replacement scene", async () => {
  const { client, engine: oldEngine, render } = await startClient();
  const configuration = observeResult(client.setPointerFillSelection(4));
  const replacement = new FakeSemanticAuthoringClient();
  const switching = client.switchToSemanticExecution("replacement", replacement);
  try {
    const rebuild = await waitForRequest(render, "rebuild_engine");
    replyRender(render, rebuild, "engine_rebuilt");
    await switching;
    assert.match((await configuration).error?.message ?? "", /retired|transition/i);
    const currentEngine = replacement.attachments.at(-1).controlPort.peer;
    assert.equal(oldEngine.messages.some(m => m.type === "pointer_fill_selection"), false);
    assert.equal(currentEngine.messages.some(m => m.type === "pointer_fill_selection"), false);
  } finally { client.terminate(); await configuration; }
});

// Collection-time receipt contract: real client, existing readiness yield and
// ownership gates. These do not claim that the fake render owner draws pixels.
async function registerPointerView(client, engine, revision = 3) {
  const delivery = client.setBrowserPointerView(revision, 800, 400);
  const sent = await waitForRequest(engine, "browser_pointer_view");
  engine.emitMessage(engineMessage("browser_pointer_view", { requestId: sent.requestId }));
  await delivery;
}
const pointerReceipt = (presentation = 1, sequence = 0, view_revision = 3, session = 1) =>
  ({ session, sequence, presentation, view_revision });
const pointerInput = { kind: "press", source_id: 1, pointer_id: 7, view_revision: 3,
  surface_x: 400, surface_y: 200, viewport_width: 800, viewport_height: 400, button: 0 };

test("pointer input pins receipt before readiness yields, despite a newer repaint", async () => {
  const { client, engine } = await startClient();
  try {
    await registerPointerView(client, engine);
    const a = pointerReceipt(), b = pointerReceipt(2, 1);
    engine.emitMessage(engineMessage("pointer_presented", { receipt: a }));
    const pending = client.submitBrowserPointerInput(pointerInput);
    engine.emitMessage(engineMessage("pointer_presented", { receipt: b }));
    const sent = await waitForRequest(engine, "browser_pointer_input");
    assert.deepEqual(sent.presentation, a, "already collected input must keep frame A");
    assert.deepEqual(client.pointerPresentation, b);
    engine.emitMessage(engineMessage(sent.type, { requestId: sent.requestId, pointerInputAccepted: false }));
    assert.equal((await pending).pointerInputAccepted, false);
  } finally { client.terminate(); }
});

test("input collected without receipt is not relabelled by a later acknowledgement", async () => {
  const { client, engine } = await startClient();
  try {
    await registerPointerView(client, engine);
    const pending = client.submitBrowserPointerInput(pointerInput);
    engine.emitMessage(engineMessage("pointer_presented", { receipt: pointerReceipt() }));
    const sent = await waitForRequest(engine, "browser_pointer_input");
    assert.equal(sent.presentation, null);
    engine.emitMessage(engineMessage(sent.type, { requestId: sent.requestId, pointerInputAccepted: false }));
    assert.equal((await pending).pointerInputAccepted, false);
  } finally { client.terminate(); }
});

test("worker receipt is immutable and neither transport consumption nor future view authorizes it", async () => {
  const { client, engine } = await startClient();
  try {
    await registerPointerView(client, engine);
    engine.emitMessage(engineMessage("execution_presented", { receipt: pointerReceipt() }));
    assert.equal(client.pointerPresentation, null);
    for (const receipt of [pointerReceipt(1, 0, 4), pointerReceipt(1, 0, 3, 2)]) {
      engine.emitMessage(engineMessage("pointer_presented", { receipt }));
      assert.equal(client.pointerPresentation, null);
    }
    const receipt = pointerReceipt();
    engine.emitMessage(engineMessage("pointer_presented", { receipt }));
    receipt.sequence = 90;
    assert.equal(client.pointerPresentation.sequence, 0);
    assert.ok(Object.isFrozen(client.pointerPresentation));
  } finally { client.terminate(); }
});

test("view registration clears receipt before any asynchronous delivery", async () => {
  const { client, engine } = await startClient();
  try {
    await registerPointerView(client, engine);
    engine.emitMessage(engineMessage("pointer_presented", { receipt: pointerReceipt() }));
    const view = client.setBrowserPointerView(4, 800, 400);
    assert.equal(client.pointerPresentation, null);
    engine.emitMessage(engineMessage("pointer_presented", { receipt: pointerReceipt(2) }));
    assert.equal(client.pointerPresentation, null, "old view feedback cannot restore mapping");
    await new Promise(resolve => setImmediate(resolve));
    const sent = requestMessage(engine, "browser_pointer_view");
    engine.emitMessage(engineMessage(sent.type, { requestId: sent.requestId }));
    await view;
  } finally { client.terminate(); }
});

test("same view is a no-op and reused revision cannot replace its geometry", async () => {
  const { client, engine } = await startClient();
  try {
    await registerPointerView(client, engine);
    const r = pointerReceipt(); engine.emitMessage(engineMessage("pointer_presented", { receipt: r }));
    await client.setBrowserPointerView(3, 800, 400);
    assert.deepEqual(client.pointerPresentation, r);
    assert.throws(() => client.setBrowserPointerView(3, 801, 400), /revision reused/);
    assert.deepEqual(client.pointerPresentation, r);
  } finally { client.terminate(); }
});

test("late invalidation cannot clear a newer successful presentation", async () => {
  const { client, engine } = await startClient();
  try {
    await registerPointerView(client, engine);
    const a=pointerReceipt(), b=pointerReceipt(2);
    engine.emitMessage(engineMessage("pointer_presented", { receipt: b }));
    engine.emitMessage(engineMessage("pointer_presentation_invalidated", { receipt: a }));
    assert.deepEqual(client.pointerPresentation, b);
    engine.emitMessage(engineMessage("pointer_presented", { receipt: a }));
    assert.deepEqual(client.pointerPresentation, b);
    engine.emitMessage(engineMessage("pointer_presentation_invalidated", { receipt: b }));
    assert.equal(client.pointerPresentation, null);
  } finally { client.terminate(); }
});

for (const operation of ["engine-restart", "semantic-replacement"]) {
  test(`${operation} must re-register an unchanged view without reusing its receipt`, async () => {
    const { client, authoring, engine: oldEngine, render } = await startClient();
    let pending;
    try {
      await registerPointerView(client, oldEngine);
      oldEngine.emitMessage(engineMessage("pointer_presented", { receipt: pointerReceipt() }));
      const replacing = operation === "engine-restart"
        ? client.restart({ failedOwner: "engine" })
        : client.switchToSemanticExecution("replacement", authoring);
      const renderRequest = await waitForRequest(render, "rebuild_engine");
      replyRender(render, renderRequest, "engine_rebuilt");
      const ready = await replacing;
      assert.equal(ready.session, 2);
      const engine = authoring.attachments.at(-1).controlPort.peer;
      assert.equal(client.pointerPresentation, null);
      const currentReceipt = pointerReceipt(2, 0, 3, 2);
      engine.emitMessage(engineMessage("pointer_presented", { receipt: currentReceipt }));
      assert.equal(client.pointerPresentation, null, "replacement needs its own platform registration");
      pending = client.setBrowserPointerView(3, 800, 400);
      const registration = await waitForRequest(engine, "browser_pointer_view");
      assert.deepEqual(registration.view, { revision: 3, width: 800, height: 400 });
      engine.emitMessage(engineMessage(registration.type, { requestId: registration.requestId }));
      await pending;
      assert.equal(client.pointerPresentation, null, "registration is not presentation");
      oldEngine.emitMessage(engineMessage("pointer_presented", { receipt: pointerReceipt(10) }));
      assert.equal(client.pointerPresentation, null);
      engine.emitMessage(engineMessage("pointer_presented", { receipt: currentReceipt }));
      assert.deepEqual(client.pointerPresentation, currentReceipt);
    } finally { client.terminate(); await pending?.catch(() => {}); }
  });
}
