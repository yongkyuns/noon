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

// The real worker reports its admitted finite endpoint independently of the
// requested loop duration. Ordinary looping/continuation replies omit it or
// report null. Keep those endpoint observations in the test transport only.
class DurationAuthoringClient extends FakeSemanticAuthoringClient {
  constructor(readyState) {
    super();
    this.readyState = readyState;
  }

  async attachSemanticExecution(contextId, controlPort, renderPort, options) {
    const post = controlPort.postMessage.bind(controlPort);
    controlPort.postMessage = message => post(message.type === "ready"
      ? { ...message, ...this.readyState } : message);
    return super.attachSemanticExecution(contextId, controlPort, renderPort, options);
  }
}

async function availableAudioDevice(t) {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  const savedDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const savedWindow = globalThis.window;
  const available = Object.getOwnPropertyDescriptor(WorkerAudioDevice.prototype, "available");
  globalThis.document = Object.assign(new EventTarget(), { hidden: false });
  globalThis.window = Object.assign(new EventTarget(), { devicePixelRatio: 1 });
  Object.defineProperty(WorkerAudioDevice.prototype, "available", { configurable: true, get: () => true });
  t.after(() => {
    if (savedDocument) Object.defineProperty(globalThis, "document", savedDocument);
    else delete globalThis.document;
    globalThis.window = savedWindow;
    Object.defineProperty(WorkerAudioDevice.prototype, "available", available);
  });
  t.mock.method(WorkerAudioDevice.prototype, "install", async () => {});
  t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", () => {});
}

async function assertSeekRange(client, authoring, end) {
  assert.equal((await client.seek(end)).time, end, "the admitted endpoint must be reachable");
  const engine = authoring.attachments.at(-1).controlPort.peer;
  const before = engine.messages.length;
  await assert.rejects(client.seek(end + 1), /exceeds loop duration/);
  assert.equal(engine.messages.length, before, "out-of-range input must not reach the worker");
}

for (const profile of [
  { name: "finite audio with a device", hasAudio: true, device: true },
  { name: "finite audio without an available device", hasAudio: true, device: false },
  { name: "finite controls without audio", hasAudio: false, device: false },
]) {
  test(`${profile.name} uses the worker endpoint at startup and replacement`, async t => {
    if (profile.device) await availableAudioDevice(t);
    const client = new ExecutionWorkerClient(new FakeCanvas());
    const authoring = new DurationAuthoringClient({ hasAudio: profile.hasAudio, durationSeconds: 30 });
    const offset = FakeWorker.instances.length;
    try {
      const starting = client.startSemanticExecution("first", authoring, { transportMode: "transferable" });
      const { render } = await finishStartup(starting, authoring, offset);
      assert.equal(authoring.attachments.at(-1).options.loopDurationSeconds, 4);
      await assertSeekRange(client, authoring, 30);

      for (const [index, endpoint, requested] of [[1, 60, null], [2, 12, 10]]) {
        const replacement = new DurationAuthoringClient({ hasAudio: profile.hasAudio, durationSeconds: endpoint });
        const switching = client.switchToSemanticExecution(`replacement-${index}`, replacement,
          { loopDurationSeconds: requested });
        replyRender(render, await waitForRequest(render, "rebuild_engine", index), "engine_rebuilt");
        const ready = await switching;
        assert.equal(ready.engine.durationSeconds, endpoint);
        assert.equal(replacement.attachments.at(-1).options.loopDurationSeconds, requested ?? 30);
        await assertSeekRange(client, replacement, endpoint);
      }
    } finally { client.terminate(); }
  });
}

for (const readyState of [{}, { durationSeconds: null }]) {
  test(`nonfinite playback retains the requested loop when endpoint is ${readyState.durationSeconds}`, async () => {
    const client = new ExecutionWorkerClient(new FakeCanvas());
    const authoring = new DurationAuthoringClient(readyState);
    const offset = FakeWorker.instances.length;
    try {
      const starting = client.startSemanticExecution("looping", authoring,
        { transportMode: "transferable", loopDurationSeconds: 10 });
      const { render } = await finishStartup(starting, authoring, offset);
      await assertSeekRange(client, authoring, 10);
      for (const [index, requested] of [[1, 6], [2, null]]) {
        const replacement = new DurationAuthoringClient(readyState);
        const switching = client.switchToSemanticExecution(`looping-${index}`, replacement,
          { loopDurationSeconds: requested });
        replyRender(render, await waitForRequest(render, "rebuild_engine", index), "engine_rebuilt");
        await switching;
        await assertSeekRange(client, replacement, 6);
      }
    } finally { client.terminate(); }
  });
}

test("a rejected replacement cannot change the current seek range", async () => {
  const client = new ExecutionWorkerClient(new FakeCanvas());
  const authoring = new DurationAuthoringClient({ durationSeconds: 30 });
  const offset = FakeWorker.instances.length;
  try {
    const starting = client.startSemanticExecution("current", authoring, { transportMode: "transferable" });
    await finishStartup(starting, authoring, offset);
    const rejected = new DurationAuthoringClient({ durationSeconds: 60 });
    rejected.failContext = "rejected";
    await assert.rejects(client.switchToSemanticExecution("rejected", rejected), /semantic context rejected/);
    await assertSeekRange(client, authoring, 30);
  } finally { client.terminate(); }
});

test("late audio preparation cannot install a retired scene's endpoint", async t => {
  await availableAudioDevice(t);
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  const client = new ExecutionWorkerClient(new FakeCanvas());
  const authoring = new DurationAuthoringClient({ hasAudio: true, durationSeconds: 30 });
  let release;
  const held = new Promise(resolve => { release = resolve; });
  let preparationStarted;
  const preparing = new Promise(resolve => { preparationStarted = resolve; });
  try {
    const offset = FakeWorker.instances.length;
    const starting = client.startSemanticExecution("first", authoring, { transportMode: "transferable" });
    const { render } = await finishStartup(starting, authoring, offset);
    let preparations = 0;
    t.mock.method(WorkerAudioDevice.prototype, "install", async () => {
      if (preparations++ === 0) {
        preparationStarted();
        await held;
      }
    });
    const retired = new DurationAuthoringClient({ hasAudio: true, durationSeconds: 60 });
    const replacing = client.switchToSemanticExecution("retired", retired);
    const cancelled = assert.rejects(replacing, /terminated during an asynchronous operation/);
    replyRender(render, await waitForRequest(render, "rebuild_engine"), "engine_rebuilt");
    await preparing;
    client.terminate();

    const fresh = new DurationAuthoringClient({ hasAudio: true, durationSeconds: 7 });
    const nextOffset = FakeWorker.instances.length;
    const restarting = client.startSemanticExecution("fresh", fresh, { transportMode: "transferable" });
    await finishStartup(restarting, fresh, nextOffset);
    release();
    await cancelled;
    await assertSeekRange(client, fresh, 7);
  } finally {
    release();
    client.terminate();
  }
});

for (const failedOwner of ["engine", "render"]) {
  test(`${failedOwner} recovery retains the admitted finite endpoint`, async () => {
    const client = new ExecutionWorkerClient(new FakeCanvas());
    const authoring = new DurationAuthoringClient({ durationSeconds: 30 });
    const offset = FakeWorker.instances.length;
    try {
      const starting = client.startSemanticExecution("finite", authoring, { transportMode: "transferable" });
      const { render } = await finishStartup(starting, authoring, offset);
      const before = FakeWorker.instances.length;
      const restarting = client.restart({ failedOwner });
      if (failedOwner === "engine") {
        replyRender(render, await waitForRequest(render, "rebuild_engine"), "engine_rebuilt");
        await restarting;
      } else {
        await finishStartup(restarting, authoring, before);
      }
      assert.equal(authoring.attachments.at(-1).options.loopDurationSeconds, 30);
      await assertSeekRange(client, authoring, 30);
    } finally { client.terminate(); }
  });
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

test("forward advance timings are explicitly opted in and invalid flags send no request", async () => {
  const { client, engine } = await startClient();
  try {
    const before = engine.messages.length;
    await assert.rejects(client.advanceTo(0.5, { collectTimings: "true" }), /must be a boolean/);
    assert.equal(engine.messages.length, before);
    for (const collectTimings of [false, true]) {
      const pending = client.advanceTo(0.5, { collectTimings });
      const message = await waitForRequest(engine, "advance_to", collectTimings ? 2 : 1);
      assert.equal(message.time, 0.5);
      assert.equal(message.collectTimings, collectTimings ? true : undefined);
      const sampleTiming = collectTimings ? { endpointMs: 3 } : undefined;
      engine.emitMessage(engineMessage("advance_to", {
        requestId: message.requestId, time: 0.5, playing: false,
        ...(collectTimings ? { sampleTiming } : {}),
      }));
      assert.equal((await pending).sampleTiming, sampleTiming);
    }
  } finally { client.terminate(); }
});

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

test("finite completion probe forwards its original bound and preserves Rust's actual endpoint", async () => {
  const { client, engine } = await startClient();
  try {
    const requested = 4 + 1e-9;
    const before = engine.messages.length;
    await assert.rejects(client.advanceTo(requested), /exceeds loop duration/);
    await assert.rejects(client.advanceTo(requested, { stopAtSourceCompletion: "true" }), /must be a boolean/);
    assert.equal(engine.messages.length, before);
    const pending = client.advanceTo(requested, { stopAtSourceCompletion: true });
    const message = await waitForRequest(engine, "advance_to");
    assert.equal(message.time, requested);
    assert.equal(message.stopAtSourceCompletion, true);
    engine.emitMessage(engineMessage("advance_to", {
      requestId: message.requestId, time: 4, sourceCompleted: true, playing: false,
    }));
    const result = await pending;
    assert.equal(result.time, 4);
    assert.equal(result.sourceCompleted, true);
  } finally { client.terminate(); }
});

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

test("semantic startup forwards the initial platform view to the authoring owner", async () => {
  const client = new ExecutionWorkerClient(new FakeCanvas());
  const authoring = new FakeSemanticAuthoringClient();
  const view = { revision: 9, width: 640, height: 360 };
  const offset = FakeWorker.instances.length;
  try {
    const starting = client.startSemanticExecution("scene", authoring, {
      transportMode: "transferable", initialPointerView: view,
    });
    await finishStartup(starting, authoring, offset);
    assert.deepEqual(authoring.attachments.at(-1).options.initialPointerView, view);
    assert.equal(client.pointerPresentation, null, "registration alone does not authorize input");
    const attachment = authoring.attachments.at(-1);
    const engine = attachment.controlPort.peer;
    const receipt = { session: attachment.options.session, sequence: 0, presentation: 1, view_revision: view.revision };
    engine.emitMessage(engineMessage("pointer_presented", { receipt }));
    assert.deepEqual(client.pointerPresentation, receipt);
    await client.setBrowserPointerView(view.revision, view.width, view.height);
    assert.equal(engine.messages.filter(message => message.type === "browser_pointer_view").length, 0);
    engine.emitMessage(engineMessage("pointer_presented", { receipt: { ...receipt, view_revision: view.revision + 1, presentation: 2 } }));
    assert.deepEqual(client.pointerPresentation, receipt, "an unregistered view cannot replace the initial receipt");
  } finally { client.terminate(); }
});

test("invalid initial pointer views fail before browser startup", async () => {
  const client = new ExecutionWorkerClient(new FakeCanvas());
  const offset = FakeWorker.instances.length;
  const authoring = new FakeSemanticAuthoringClient();
  try {
    for (const view of [
      { revision: -1, width: 640, height: 360 },
      { revision: 1, width: NaN, height: 360 },
      { revision: 1, width: 640, height: -1 },
    ]) {
      await assert.rejects(client.startSemanticExecution("scene", authoring, {
        initialPointerView: view,
      }), /invalid or retired browser pointer view/);
    }
    assert.equal(FakeWorker.instances.length, offset);
    assert.equal(authoring.attachments.length, 0);
  } finally { client.terminate(); }
});

test("engine and render requests use independent issuance spaces", async () => {
  const errors = [];
  const { client, engine, render } = await startClient(errors);

  const metricsPromise = client.metrics();
  await Promise.resolve();
  const engineMetrics = requestMessage(engine, "metrics");
  const renderMetrics = requestMessage(render, "metrics");
  assert.deepEqual(renderMetrics, {
    channel: "noon.render", protocolVersion: 1, type: "metrics", requestId: 2,
    profilePublicationStages: false, profileRenderSubstages: false,
    profileGpuCompletion: false, includeGpuIdentity: false,
  });
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

test("retired source retains real renderer metrics without querying its released engine", async () => {
  const { client, engine, render } = await startClient();
  await client.retireExecutionSource();
  assert.equal(engine.closed, true);
  assert.equal(render.terminated, false);
  await assert.rejects(client.state(), /has not been started/);
  await assert.rejects(client.seek(0), /has not been started/);
  const engineMessages = engine.messages.length;
  const reportPromise = client.metrics();
  reportPromise.catch(() => {});
  const request = await waitForRequest(render, "metrics");
  render.emitMessage(renderMessage("metrics", {
    requestId: request.requestId,
    metrics: { ready: true, presentedFrames: 17, objectCount: 2 },
  }));
  const report = await reportPromise;
  assert.deepEqual(report.metrics, { ready: true, presentedFrames: 17, objectCount: 2 });
  assert.equal(report.engineMetrics, null, "released engine observations are unavailable");
  assert.equal(engine.messages.length, engineMessages);
  client.terminate();
  await assert.rejects(client.metrics(), /has not been started/);
});

test("render metrics options, including one-shot GPU identity, are explicit", async () => {
  const errors = [];
  const { client, engine, render } = await startClient(errors);
  await assert.rejects(client.metrics({ profileGpuCompletion: 1 }), /must be booleans/);
  const metricsPromise = client.metrics({ profileRenderSubstages: true,
    profileGpuCompletion: true, includeGpuIdentity: true });
  await Promise.resolve();
  const engineMetrics = requestMessage(engine, "metrics");
  const renderMetrics = requestMessage(render, "metrics");
  assert.equal(renderMetrics.profilePublicationStages, false);
  assert.equal(renderMetrics.profileRenderSubstages, true);
  assert.equal(renderMetrics.profileGpuCompletion, true);
  assert.equal(renderMetrics.includeGpuIdentity, true);
  engine.emitMessage(engineMessage("metrics", { requestId: engineMetrics.requestId, metrics: {} }));
  render.emitMessage(renderMessage("metrics", { requestId: renderMetrics.requestId, metrics: {} }));
  await metricsPromise;
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
    engineMessage("pause", {
      requestId: initialPause.requestId,
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

const inspectionInput = { view_revision: 3, viewport_width: 800, viewport_height: 400,
  surface_x: 500, surface_y: 200, delta_pixels: -100 };

for (const collected of [null, pointerReceipt()]) {
  test(`inspection pins its receipt and normalized cursor before yielding (${collected === null ? "unpresented" : "presented"})`, async () => {
    const { client, engine } = await startClient();
    try {
      await registerPointerView(client, engine);
      if (collected) engine.emitMessage(engineMessage("pointer_presented", { receipt: collected }));
      const input = { ...inspectionInput };
      const pending = client.scrollInspectionView(input);
      input.surface_x = 9;
      engine.emitMessage(engineMessage("pointer_presented", { receipt: pointerReceipt(2, 1) }));
      const sent = await waitForRequest(engine, "inspection_scroll");
      assert.deepEqual(sent.input, inspectionInput);
      assert.deepEqual(sent.presentation, collected);
      engine.emitMessage(engineMessage(sent.type, { requestId: sent.requestId, inspectionScrollChanged: null }));
      assert.equal((await pending).inspectionScrollChanged, null);
    } finally { client.terminate(); }
  });
}

test("inspection shares native ingress capacity and never evicts pending pointer input", async () => {
  const { client, engine } = await startClient(); const pending = [];
  try {
    for (let i = 0; i < MAX_IN_FLIGHT_NATIVE_INPUTS; i++) {
      pending.push(observeResult(client.scrollInspectionView(inspectionInput)));
    }
    await assert.rejects(client.submitBrowserPointerInput(pointerInput), /in-flight capacity is full/);
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(engine.messages.filter(m => m.type === "inspection_scroll").length, MAX_IN_FLIGHT_NATIVE_INPUTS);
    assert.equal(engine.messages.filter(m => m.type === "browser_pointer_input").length, 0);
  } finally { client.terminate(); await Promise.all(pending); }
});

test("inspection reserved before termination is never delivered to a retired endpoint", async () => {
  const { client, engine } = await startClient();
  const pending = observeResult(client.scrollInspectionView(inspectionInput));
  client.terminate();
  assert.match((await pending).error?.message ?? "", /retired|transition|terminated/i);
  assert.equal(engine.messages.filter(m => m.type === "inspection_scroll").length, 0);
});

test("audio timing uses the matched response once and ignores duplicate or unissued responses", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  const applied = [], errors = [];
  t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", (anchor, session, bracket) => {
    applied.push({ anchor, session, bracket });
  });
  const { client, engine } = await startClient(errors);
  let now = 100;
  t.mock.method(performance, "now", () => now);
  try {
    const playing = client.resume();
    const request = await waitForRequest(engine, "resume");
    now = 106;
    const anchor = { session: 1, playing: true, time: 2.1, sampledEpochMs: 9999 };
    const response = engineMessage("resume", { requestId: request.requestId, audioAnchor: anchor });
    engine.emitMessage(response);
    now = 110;
    await playing;
    assert.equal(applied.length, 1);
    assert.equal(applied[0].anchor, anchor);
    assert.equal(applied[0].bracket.sentMs, 100);
    assert.equal(applied[0].bracket.receivedMs, 106,
      "receipt is read at settlement, before the later Promise continuation");
    assert.equal(applied[0].bracket.requestId, request.requestId);
    assert.equal(applied[0].bracket.worker, engine);
    assert.equal(applied[0].bracket.session, 1);
    assert.equal(Object.getOwnPropertySymbols({ ...response }).length, 0,
      "host-only timing metadata must not become enumerable wire data");
    engine.emitMessage(response);
    assert.equal(client.diagnostics.engine.staleResponses, 1);
    engine.emitMessage(engineMessage("resume", { requestId: request.requestId + 1, audioAnchor: anchor }));
    assert.equal(errors.length, 1);
    assert.match(errors[0], /unissued request ID/);
    assert.equal(applied.length, 1);
  } finally { client.terminate(); }
});

test("an issued ID with the wrong response type cannot authorize audio timing", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  const applied = [];
  t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", (...args) => applied.push(args));
  const { client, engine } = await startClient();
  try {
    const playing = client.resume();
    const rejected = assert.rejects(playing, /does not match its issued command/);
    const request = await waitForRequest(engine, "resume");
    engine.emitMessage(engineMessage("seek", { requestId: request.requestId,
      audioAnchor: { session: 1, playing: true, time: 2.1, sampledEpochMs: 9999 } }));
    await rejected;
    assert.deepEqual(applied, []);
    assert.equal(client.diagnostics.engine.pendingRequests, 0);
  } finally { client.terminate(); }
});

test("a late response from a retired audio session cannot acquire a fresh timing bracket", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  const applied = [];
  t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", (anchor, session, bracket) => {
    applied.push({ anchor, session, bracket });
  });
  const { client, authoring, engine: oldEngine } = await startClient();
  try {
    const playing = client.resume();
    const rejected = assert.rejects(playing, /restart|terminated|retired/i);
    const oldRequest = await waitForRequest(oldEngine, "resume");
    const offset = FakeWorker.instances.length;
    const restarting = client.restart();
    const { engine } = await finishStartup(restarting, authoring, offset);
    await rejected;
    oldEngine.emitMessage(engineMessage("resume", { requestId: oldRequest.requestId,
      audioAnchor: { session: 1, playing: true, time: 2.1, sampledEpochMs: 9999 } }));
    assert.deepEqual(applied, []);
    const fresh = client.resume();
    const request = await waitForRequest(engine, "resume");
    engine.emitMessage(engineMessage("resume", { requestId: request.requestId,
      audioAnchor: { session: 2, playing: true, time: 0, sampledEpochMs: 10000 } }));
    await fresh;
    assert.equal(applied.length, 1);
    assert.equal(applied[0].bracket.session, 2);
    assert.equal(applied[0].bracket.worker, engine);
    assert.equal(applied[0].bracket.requestId, request.requestId);
  } finally { client.terminate(); }
});

test("a retired device unlock cannot resume its former worker", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  let completeUnlock;
  const unlock = new Promise(resolve => { completeUnlock = resolve; });
  let unlockCalls = 0;
  t.mock.method(WorkerAudioDevice.prototype, "unlock", () => { unlockCalls += 1; return unlock; });
  const { client, engine } = await startClient();
  const resumed = client.resume();
  assert.equal(unlockCalls, 1, "activation starts synchronously in the caller's turn");
  client.terminate();
  completeUnlock();
  await assert.rejects(resumed, error => error.name === "AbortError");
  assert.equal(engine.messages.some(entry => (entry.message ?? entry).type === "resume"), false);
});

test("pause supersedes an outstanding device unlock without a delayed restart", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  let completeUnlock;
  const unlock = new Promise(resolve => { completeUnlock = resolve; });
  t.mock.method(WorkerAudioDevice.prototype, "unlock", () => unlock);
  const { client, engine } = await startClient();
  try {
    const resumed = client.resume();
    const paused = client.pause();
    const request = await waitForRequest(engine, "pause");
    engine.emitMessage(engineMessage("pause", { requestId: request.requestId, time: 0, playing: false }));
    assert.equal((await paused).playing, false);
    completeUnlock();
    await assert.rejects(resumed, error => error.name === "AbortError");
    assert.equal(engine.messages.some(entry => (entry.message ?? entry).type === "resume"), false);
  } finally { client.terminate(); }
});

test("scene Play unlock starts in the release turn and completion preserves the original pointer acknowledgement", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  let finishUnlock, unlockCalls = 0;
  const unlocked = new Promise(resolve => { finishUnlock = resolve; });
  const anchors = [];
  t.mock.method(WorkerAudioDevice.prototype, "unlock", () => { unlockCalls += 1; return unlocked; });
  t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", (anchor, session, bracket) => anchors.push({ anchor, session, bracket }));
  const { client, engine } = await startClient();
  try {
    const clicked = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
    assert.equal(unlockCalls, 1, "device activation begins before any readiness or worker yield");
    const input = await waitForRequest(engine, "browser_pointer_input");
    engine.emitMessage(engineMessage(input.type, { requestId: input.requestId, time: 0, playing: false,
      pointerInputAccepted: true, sceneControl: { sequence: "9007199254740993", action: "toggle_playback" } }));
    await Promise.resolve();
    await Promise.resolve();
    assert.equal(engine.messages.some(message => message.type === "scene_control"), false);
    finishUnlock();
    const completion = await waitForRequest(engine, "scene_control");
    assert.equal(completion.sequence, "9007199254740993", "u64 input receipts never round through Number");
    const anchor = { session: 1, playing: true, time: 0, sampledEpochMs: 1234 };
    const picture = { session: 1, sequence: 2, presentation: 3, view_revision: 11 };
    engine.emitMessage(engineMessage("scene_control", { requestId: completion.requestId, time: 0, playing: true,
      sceneControlApplied: true, sceneControlAction: "toggle_playback", audioAnchor: anchor,
      sceneControlPresentation: picture }));
    const result = await clicked;
    assert.equal(result.type, "browser_pointer_input");
    assert.equal(result.pointerInputAccepted, true);
    assert.equal(result.sceneControlApplied, true);
    assert.equal(result.playing, true);
    assert.deepEqual(result.sceneControlPresentation, picture);
    assert.equal(anchors.length, 1);
    assert.equal(anchors[0].anchor, anchor);
    assert.equal(anchors[0].session, 1);
    const bracket = anchors[0].bracket;
    assert.equal(bracket.requestId, completion.requestId,
      "the spread result must retain the actual scene-control sample, not the original release");
    assert.equal(bracket.type, "scene_control");
    assert.equal(bracket.session, 1);
    assert.ok(bracket.sentMs <= bracket.receivedMs);
    assert.ok(Object.isFrozen(bracket));
  } finally { client.terminate(); }
});

test("pause cancels pending scene Play by exact sequence and its delayed unlock finishes nonfatally", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  let finishUnlock;
  const unlocked = new Promise(resolve => { finishUnlock = resolve; });
  t.mock.method(WorkerAudioDevice.prototype, "unlock", () => unlocked);
  const { client, engine } = await startClient();
  try {
    const clicked = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
    const input = await waitForRequest(engine, "browser_pointer_input");
    engine.emitMessage(engineMessage(input.type, { requestId: input.requestId, time: 0, playing: false,
      pointerInputAccepted: true, sceneControl: { sequence: "7", action: "toggle_playback" } }));
    await Promise.resolve();
    await Promise.resolve();
    const pausing = client.pause();
    const cancel = await waitForRequest(engine, "scene_control");
    assert.equal(cancel.sequence, "7");
    assert.equal(cancel.cancel, true);
    engine.emitMessage(engineMessage(cancel.type, { requestId: cancel.requestId, sceneControlApplied: false }));
    const pause = await waitForRequest(engine, "pause");
    engine.emitMessage(engineMessage(pause.type, { requestId: pause.requestId, time: 0, playing: false }));
    await pausing;
    finishUnlock();
    const result = await clicked;
    assert.equal(result.pointerInputAccepted, true);
    assert.equal(result.sceneControlApplied, false);
    assert.equal(engine.messages.filter(message => message.type === "scene_control" && !message.cancel).length, 0);
  } finally { client.terminate(); }
});

test("focus loss settles pending scene Play before its held audio unlock resolves", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  let finishUnlock, released;
  const unlocked = new Promise(resolve => { finishUnlock = resolve; });
  const anchors = [];
  t.mock.method(WorkerAudioDevice.prototype, "unlock", () => unlocked);
  t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", anchor => anchors.push(anchor));
  const { client, engine } = await startClient();
  let clicked;
  try {
    clicked = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
    void clicked.then(result => { released = result; }, () => {});
    const input = await waitForRequest(engine, "browser_pointer_input");
    engine.emitMessage(engineMessage(input.type, { requestId: input.requestId, time: 0, playing: false,
      pointerInputAccepted: true, sceneControl: { sequence: "8", action: "toggle_playback" } }));
    await Promise.resolve();
    await Promise.resolve();
    const lostFocus = client.submitBrowserPointerInput({ ...pointerInput, kind: "focus_lost" });
    const lost = await waitForRequest(engine, "browser_pointer_input", 2);
    engine.emitMessage(engineMessage(lost.type, { requestId: lost.requestId, time: 0, playing: false,
      pointerInputAccepted: true }));
    await lostFocus;
    assert.equal(released?.sceneControlApplied, false,
      "the old release must settle without waiting for the audio unlock");
    assert.equal(released.pointerInputAccepted, true);
    const cancel = await waitForRequest(engine, "scene_control");
    assert.equal(cancel.sequence, "8");
    assert.equal(cancel.cancel, true);
    engine.emitMessage(engineMessage(cancel.type, { requestId: cancel.requestId, sceneControlApplied: false }));
    finishUnlock();
    await clicked;
    await Promise.resolve();
    assert.deepEqual(anchors, []);
    assert.equal(engine.messages.filter(message => message.type === "scene_control" && !message.cancel).length, 0);
  } finally {
    finishUnlock();
    client.terminate();
    await clicked?.catch(() => {});
  }
});

test("a new committed scene action retires an older external Play unlock", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  let finishUnlock;
  const unlocked = new Promise(resolve => { finishUnlock = resolve; });
  t.mock.method(WorkerAudioDevice.prototype, "unlock", () => unlocked);
  const anchors = [];
  t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", anchor => anchors.push(anchor));
  const { client, engine } = await startClient();
  try {
    const resumed = client.resume();
    const rejected = assert.rejects(resumed, error => error.name === "AbortError");
    const clicked = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
    const input = await waitForRequest(engine, "browser_pointer_input");
    const anchor = { session: 1, time: 0, playing: false, sampledEpochMs: 1234 };
    engine.emitMessage(engineMessage(input.type, { requestId: input.requestId, time: 0, playing: false,
      pointerInputAccepted: true, sceneControlApplied: true, sceneControlAction: "restart", audioAnchor: anchor }));
    assert.equal((await clicked).sceneControlApplied, true);
    finishUnlock();
    await rejected;
    assert.deepEqual(anchors, [anchor]);
    assert.equal(engine.messages.some(message => message.type === "resume"), false);
  } finally { client.terminate(); }
});

test("ordered scene acknowledgements survive earlier scene actions and a later uncompleted press", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  const anchors = [];
  t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", anchor => anchors.push(anchor));
  const { client, engine } = await startClient();
  try {
    const first = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
    const second = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
    const pressed = client.submitBrowserPointerInput(pointerInput);
    const requests = [await waitForRequest(engine, "browser_pointer_input", 1),
      await waitForRequest(engine, "browser_pointer_input", 2),
      await waitForRequest(engine, "browser_pointer_input", 3)];
    const expected = [{ session: 1, time: 1, playing: true, sampledEpochMs: 100 },
      { session: 1, time: 1, playing: false, sampledEpochMs: 101 }];
    for (let index = 0; index < 2; index++) {
      engine.emitMessage(engineMessage("browser_pointer_input", { requestId: requests[index].requestId,
        time: 1, playing: expected[index].playing, pointerInputAccepted: true, sceneControlApplied: true,
        sceneControlAction: index === 0 ? "next_section" : "toggle_playback", audioAnchor: expected[index] }));
    }
    engine.emitMessage(engineMessage("browser_pointer_input", { requestId: requests[2].requestId,
      pointerInputAccepted: true }));
    await Promise.all([first, second, pressed]);
    assert.deepEqual(anchors, expected, "a press may cancel pending unlock, not an already committed Pause");
  } finally { client.terminate(); }
});

test("scene mute changes only viewer gain and an older mute reply cannot override a newer explicit gain", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  const pauses = [], anchors = [], mutes = [];
  const setMuted = WorkerAudioDevice.prototype.setMuted;
  t.mock.method(WorkerAudioDevice.prototype, "pause", () => pauses.push(true));
  t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", anchor => anchors.push(anchor));
  t.mock.method(WorkerAudioDevice.prototype, "setMuted", function(value) {
    mutes.push(value); setMuted.call(this, value);
  });
  const { client, engine } = await startClient();
  try {
    const first = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
    const request1 = await waitForRequest(engine, "browser_pointer_input");
    engine.emitMessage(engineMessage(request1.type, { requestId: request1.requestId, time: 1, playing: true,
      pointerInputAccepted: true, sceneControlApplied: true, sceneControlAction: "toggle_audio_muted" }));
    await first;
    assert.deepEqual(mutes, [true]);
    assert.deepEqual(pauses, []);
    assert.deepEqual(anchors, []);
    const second = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
    const request2 = await waitForRequest(engine, "browser_pointer_input", 2);
    client.setAudioMuted(false);
    engine.emitMessage(engineMessage(request2.type, { requestId: request2.requestId, time: 1, playing: true,
      pointerInputAccepted: true, sceneControlApplied: true, sceneControlAction: "toggle_audio_muted" }));
    await second;
    assert.deepEqual(mutes, [true, false], "the later explicit value wins over the older asynchronous toggle");
    assert.deepEqual(anchors, []);
  } finally { client.terminate(); }
});

test("rejected pointer release cannot apply controls and an unused unlock rejection is observed", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  t.mock.method(WorkerAudioDevice.prototype, "unlock", () => Promise.reject(new Error("device denied")));
  const { client, engine } = await startClient();
  try {
    const clicked = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
    const input = await waitForRequest(engine, "browser_pointer_input");
    engine.emitMessage(engineMessage(input.type, { requestId: input.requestId, pointerInputAccepted: false }));
    assert.equal((await clicked).pointerInputAccepted, false);
    assert.equal(engine.messages.some(message => message.type === "scene_control"), false);
  } finally { client.terminate(); }
});

test("hiding the page pauses a worker resume whose acknowledgement has not updated the cached state", async t => {
  const { WorkerAudioDevice } = await import("./worker-audio-device.js");
  const savedDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const savedWindow = globalThis.window;
  const available = Object.getOwnPropertyDescriptor(WorkerAudioDevice.prototype, "available");
  const document = Object.assign(new EventTarget(), { hidden: false });
  globalThis.document = document;
  globalThis.window = Object.assign(new EventTarget(), { devicePixelRatio: 1 });
  Object.defineProperty(WorkerAudioDevice.prototype, "available", { configurable: true, get: () => true });
  t.after(() => {
    if (savedDocument) Object.defineProperty(globalThis, "document", savedDocument);
    else delete globalThis.document;
    globalThis.window = savedWindow;
    Object.defineProperty(WorkerAudioDevice.prototype, "available", available);
  });
  t.mock.method(WorkerAudioDevice.prototype, "install", async () => {});
  t.mock.method(WorkerAudioDevice.prototype, "unlock", () => Promise.resolve());
  const anchors = [];
  t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", anchor => { if (anchor) anchors.push(anchor); });
  const { client, engine } = await startClient();
  try {
    const pausing = client.pause();
    const firstPause = await waitForRequest(engine, "pause");
    engine.emitMessage(engineMessage("pause", { requestId: firstPause.requestId, playing: false, time: 0 }));
    await pausing;
    const resuming = client.resume();
    const resume = await waitForRequest(engine, "resume");
    document.hidden = true;
    document.dispatchEvent(new Event("visibilitychange"));
    const hiddenPause = await waitForRequest(engine, "pause", 2);
    engine.emitMessage(engineMessage("resume", { requestId: resume.requestId, playing: true, time: 0,
      audioAnchor: { session: 1, playing: true, time: 0, sampledEpochMs: 1234 } }));
    engine.emitMessage(engineMessage("pause", { requestId: hiddenPause.requestId, playing: false, time: 0 }));
    await resuming;
    assert.deepEqual(anchors, [], "an in-flight resume cannot re-enable output after page suspension");
  } finally { client.terminate(); }
});


const viewerGainChanges = [
  { name: "volume", apply: client => client.setAudioVolume(0.25), volume: 0.25, muted: false },
  { name: "mute", apply: client => client.setAudioMuted(true), volume: 1, muted: true },
];

for (const gain of viewerGainChanges) {
  for (const action of [
    { name: "Pause", action: "toggle_playback", time: 1, playing: false },
    { name: "section seek", action: "next_section", time: 2, playing: true },
  ]) {
    test(`explicit viewer ${gain.name} preserves a committed scene ${action.name} anchor`, async t => {
      const { WorkerAudioDevice } = await import("./worker-audio-device.js");
      const anchors = [], appliedGains = [];
      t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", function(anchor) {
        anchors.push(anchor);
        appliedGains.push({ volume: this.volume, muted: this.muted });
      });
      const { client, engine } = await startClient();
      try {
        const clicked = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
        const request = await waitForRequest(engine, "browser_pointer_input");
        gain.apply(client);
        const anchor = { session: 1, time: action.time, playing: action.playing, sampledEpochMs: 1234 };
        engine.emitMessage(engineMessage(request.type, { requestId: request.requestId,
          time: action.time, playing: action.playing, pointerInputAccepted: true,
          sceneControlApplied: true, sceneControlAction: action.action, audioAnchor: anchor }));
        const result = await clicked;
        assert.deepEqual(anchors, [anchor], "gain changes cannot discard a committed transport action");
        assert.equal(result.sceneControlApplied, true);
        assert.equal(result.playing, action.playing);
        assert.equal(result.time, action.time);
        assert.deepEqual(appliedGains, [{ volume: gain.volume, muted: gain.muted }]);
      } finally { client.terminate(); }
    });
  }

  test(`explicit viewer ${gain.name} preserves an external Play anchor after worker dispatch`, async t => {
    const { WorkerAudioDevice } = await import("./worker-audio-device.js");
    const anchors = [], appliedGains = [];
    t.mock.method(WorkerAudioDevice.prototype, "unlock", () => Promise.resolve());
    t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", function(anchor) {
      anchors.push(anchor);
      appliedGains.push({ volume: this.volume, muted: this.muted });
    });
    const { client, engine } = await startClient();
    try {
      const playing = client.resume();
      const request = await waitForRequest(engine, "resume");
      gain.apply(client);
      const anchor = { session: 1, time: 1, playing: true, sampledEpochMs: 1234 };
      engine.emitMessage(engineMessage(request.type, { requestId: request.requestId,
        time: 1, playing: true, audioAnchor: anchor }));
      const result = await playing;
      assert.deepEqual(anchors, [anchor], "a sent transport command must retain its device acknowledgement");
      assert.equal(result.playing, true);
      assert.deepEqual(appliedGains, [{ volume: gain.volume, muted: gain.muted }]);
    } finally { client.terminate(); }
  });

  test(`explicit viewer ${gain.name} cancels a held scene Play before unlock`, async t => {
    const { WorkerAudioDevice } = await import("./worker-audio-device.js");
    let finishUnlock, released;
    const unlocked = new Promise(resolve => { finishUnlock = resolve; });
    const anchors = [];
    t.mock.method(WorkerAudioDevice.prototype, "unlock", () => unlocked);
    t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", anchor => anchors.push(anchor));
    const { client, engine } = await startClient();
    let clicked;
    try {
      clicked = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
      void clicked.then(result => { released = result; }, () => {});
      const input = await waitForRequest(engine, "browser_pointer_input");
      engine.emitMessage(engineMessage(input.type, { requestId: input.requestId, time: 0, playing: false,
        pointerInputAccepted: true, sceneControl: { sequence: "41", action: "toggle_playback" } }));
      await Promise.resolve();
      await Promise.resolve();
      gain.apply(client);
      const cancel = await waitForRequest(engine, "scene_control");
      assert.equal(cancel.sequence, "41");
      assert.equal(cancel.cancel, true);
      engine.emitMessage(engineMessage(cancel.type, { requestId: cancel.requestId, sceneControlApplied: false }));
      await new Promise(resolve => setImmediate(resolve));
      assert.equal(released?.sceneControlApplied, false,
        "the release must settle nonfatally before its device unlock completes");
      assert.equal(released.pointerInputAccepted, true);
      finishUnlock();
      await clicked;
      await new Promise(resolve => setImmediate(resolve));
      assert.deepEqual(anchors, []);
      assert.equal(engine.messages.filter(message => message.type === "scene_control" && !message.cancel).length, 0);
      assert.equal(engine.messages.some(message => message.type === "resume"), false);
    } finally {
      finishUnlock();
      client.terminate();
      await clicked?.catch(() => {});
    }
  });
}

for (const later of [
  { name: "press", apply: client => client.submitBrowserPointerInput(pointerInput), volume: 1, muted: false },
  ...viewerGainChanges,
]) {
  for (const applied of [true, false]) {
    test(`${applied ? "committed" : "unapplied"} scene Play survives a later ${later.name} after worker dispatch`, async t => {
      const { WorkerAudioDevice } = await import("./worker-audio-device.js");
      const anchors = [], appliedGains = [];
      t.mock.method(WorkerAudioDevice.prototype, "unlock", () => Promise.resolve());
      t.mock.method(WorkerAudioDevice.prototype, "applyAnchor", function(anchor) {
        anchors.push(anchor);
        appliedGains.push({ volume: this.volume, muted: this.muted });
      });
      const { client, engine } = await startClient();
      let clicked, pressed;
      try {
        clicked = client.submitBrowserPointerInput({ ...pointerInput, kind: "release" });
        const input = await waitForRequest(engine, "browser_pointer_input");
        engine.emitMessage(engineMessage(input.type, { requestId: input.requestId, time: 0, playing: false,
          pointerInputAccepted: true, sceneControl: { sequence: "42", action: "toggle_playback" } }));
        const completion = await waitForRequest(engine, "scene_control");
        assert.equal(completion.sequence, "42");
        assert.notEqual(completion.cancel, true);
        pressed = later.apply(client);
        const cancel = await waitForRequest(engine, "scene_control", 2);
        assert.equal(cancel.sequence, "42");
        assert.equal(cancel.cancel, true);
        const anchor = { session: 1, time: 0, playing: true, sampledEpochMs: 1234 };
        engine.emitMessage(engineMessage(completion.type, { requestId: completion.requestId,
          time: 0, playing: applied, sceneControlApplied: applied,
          ...(applied ? { sceneControlAction: "toggle_playback", audioAnchor: anchor } : {}) }));
        engine.emitMessage(engineMessage(cancel.type, { requestId: cancel.requestId,
          time: 0, playing: applied, sceneControlApplied: false }));
        if (later.name === "press") {
          const press = await waitForRequest(engine, "browser_pointer_input", 2);
          engine.emitMessage(engineMessage(press.type, { requestId: press.requestId,
            time: 0, playing: applied, pointerInputAccepted: true }));
          await pressed;
        }
        const result = await clicked;
        assert.deepEqual(anchors, applied ? [anchor] : [],
          "the worker's committed result decides whether this reply applies a device anchor");
        assert.equal(result.type, "browser_pointer_input");
        assert.equal(result.pointerInputAccepted, true);
        assert.equal(result.sceneControlApplied, applied);
        assert.equal(result.playing, applied);
        assert.deepEqual(appliedGains, applied ? [{ volume: later.volume, muted: later.muted }] : []);
      } finally {
        client.terminate();
        await clicked?.catch(() => {});
        await pressed?.catch(() => {});
      }
    });
  }
}
