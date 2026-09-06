import assert from "node:assert/strict";
import test from "node:test";

class FakeCanvas {
  clientWidth = 640;
  clientHeight = 360;
  width = 640;
  height = 360;
  transferControlToOffscreen() {
    return { width: this.width, height: this.height };
  }
  cloneNode() {
    return new FakeCanvas();
  }
  replaceWith() {}
}

class FakePort {
  listeners = new Map();
  messages = [];
  peer = null;
  closed = false;
  started = false;
  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }
  postMessage(message) {
    this.messages.push(message);
    queueMicrotask(() => this.peer?.emitMessage(message));
  }
  start() {
    this.started = true;
  }
  close() {
    this.closed = true;
  }
  emitMessage(message) {
    for (const listener of this.listeners.get("message") ?? []) {
      listener({ data: message });
    }
  }
}

class FakeMessageChannel {
  constructor() {
    this.port1 = new FakePort();
    this.port2 = new FakePort();
    this.port1.peer = this.port2;
    this.port2.peer = this.port1;
  }
}

class FakeWorker {
  static instances = [];
  listeners = new Map();
  messages = [];
  terminated = false;
  constructor(_url, options = {}) {
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
globalThis.MessageChannel = FakeMessageChannel;
globalThis.Worker = FakeWorker;
globalThis.window = { devicePixelRatio: 1 };

const {
  AuthoringExecutionClient,
  AUTHORING_EXECUTION_SEMANTIC,
} = await import("./authoring-execution-client.js");

function envelope(channel, type, payload = {}) {
  return { channel, protocolVersion: 1, type, ...payload };
}

function renderWorker() {
  return FakeWorker.instances.findLast((worker) => worker.name === "noon-render");
}

function request(worker, type) {
  const entry = worker.messages.findLast(({ message }) => message.type === type);
  assert.ok(entry, `missing ${type}`);
  return entry.message;
}

async function waitForRequest(worker, type) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const entry = worker.messages.findLast(({ message }) => message.type === type);
    if (entry) return entry.message;
    await Promise.resolve();
  }
  assert.fail(`missing ${type}`);
}

function replyRender(worker, type, responseType, payload = {}) {
  const sent = request(worker, type);
  worker.emitMessage(
    envelope("noon.render", responseType, {
      requestId: sent.requestId,
      transportMode: "transferable",
      backend: "WebGL2",
      ...payload,
    }),
  );
}

class FakeSemanticAuthoringClient {
  attachments = [];
  stoppedContexts = [];
  releasedContexts = [];
  failContext = null;

  async attachSemanticExecution(contextId, controlPort, renderPort, options) {
    this.attachments.push({ contextId, controlPort, renderPort, options });
    if (contextId === this.failContext) {
      throw new Error("semantic context rejected");
    }
    controlPort.addEventListener("message", ({ data: message }) => {
      if (message.type === "stop") {
        this.stoppedContexts.push(contextId);
        return;
      }
      const state = {
        requestId: message.requestId,
        time: message.type === "seek" ? message.time : 0,
        playing: message.type === "pause" ? false : true,
        nextPatchSequence: "1",
      };
      controlPort.postMessage(envelope("noon.engine", message.type, state));
    });
    controlPort.start();
    // The real Python endpoint queues transport setup and sequence-zero snapshot
    // on renderPort before it publishes readiness on this control endpoint.
    renderPort.postMessage({ type: "test-sequence-zero-snapshot", session: options.session });
    controlPort.postMessage(
      envelope("noon.engine", "ready", {
        transportMode: options.transportMode,
        semantic: true,
      }),
    );
    return { type: "semantic_execution_attached", contextId };
  }

  async releaseSemanticExecution(contextId) {
    this.releasedContexts.push(contextId);
    return { type: "semantic_execution_released", contextId };
  }
}

async function prepare(client) {
  const prepared = client.prepare({ transportMode: "transferable" });
  const render = renderWorker();
  replyRender(render, "prepare", "prepared");
  await prepared;
  return render;
}

test("semantic startup uses context-owned ports without constructing an engine worker", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const render = await prepare(client);
  const started = client.startSemanticExecution(
    { contextId: "semantic-1" },
    { authoringClient: authoring, loopDurationSeconds: 2, initiallyPaused: true },
  );
  await Promise.resolve();
  replyRender(render, "start_engine", "engine_started", { mode: "legacy" });
  const ready = await started;

  assert.equal(client.mode, AUTHORING_EXECUTION_SEMANTIC);
  assert.equal(ready.session, 1);
  assert.equal(authoring.attachments.length, 1);
  assert.equal(authoring.attachments[0].options.session, 1);
  assert.equal(authoring.attachments[0].options.initiallyPaused, true);
  assert.equal(authoring.attachments[0].controlPort === authoring.attachments[0].renderPort, false);
  assert.deepEqual(
    FakeWorker.instances.map(({ name }) => name),
    ["noon-render"],
    "semantic execution must not construct a legacy JSON engine worker",
  );

  const paused = client.pause();
  assert.equal((await paused).playing, false);
  const sought = client.seek(1);
  assert.equal((await sought).time, 1);
  await client.advanceToWithRendererObservation(1.25);
  const observedAdvance = authoring.attachments[0].controlPort.peer.messages.findLast(
    (message) => message.type === "advance_to",
  );
  assert.equal(observedAdvance.time, 1.25);
  assert.equal(observedAdvance.observeRenderer, true);
  assert.equal((await client.resume()).playing, true);
  client.terminate();
});

test("initially paused startup rejects a source-owned continuation before attachment", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  await assert.rejects(
    client.startSemanticExecution(
      { contextId: "semantic-continuation", continuationGeneration: 7 },
      { authoringClient: authoring, initiallyPaused: true },
    ),
    /source-owned semantic continuations cannot start paused/,
  );
  assert.equal(authoring.attachments.length, 0);
  assert.deepEqual(FakeWorker.instances, []);
  client.terminate();
});

test("semantic startup preserves shared mailbox transport options", async () => {
  FakeWorker.instances.length = 0;
  globalThis.crossOriginIsolated = true;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  try {
    const prepared = client.prepare({ transportMode: "shared", sharedSlotCapacity: 4096 });
    const render = renderWorker();
    replyRender(render, "prepare", "prepared", { transportMode: "shared" });
    await prepared;
    const started = client.startSemanticExecution(
      { contextId: "semantic-shared" },
      { authoringClient: authoring, sharedSlotCapacity: 4096 },
    );
    await Promise.resolve();
    replyRender(render, "start_engine", "engine_started", {
      mode: "legacy",
      transportMode: "shared",
    });
    const ready = await started;
    assert.equal(ready.transportMode, "shared");
    assert.deepEqual(
      {
        transportMode: authoring.attachments[0].options.transportMode,
        sharedSlotCapacity: authoring.attachments[0].options.sharedSlotCapacity,
      },
      { transportMode: "shared", sharedSlotCapacity: 4096 },
    );
  } finally {
    client.terminate();
    globalThis.crossOriginIsolated = false;
  }
});

test("semantic rerun preflights its context before switching the live renderer", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const render = await prepare(client);
  const initial = client.startSemanticExecution(
    { contextId: "semantic-1" },
    { authoringClient: authoring },
  );
  await Promise.resolve();
  replyRender(render, "start_engine", "engine_started", { mode: "legacy" });
  await initial;

  authoring.failContext = "semantic-bad";
  await assert.rejects(
    client.reconcileSemanticExecution(
      { contextId: "semantic-bad" },
      { authoringClient: authoring },
    ),
    /semantic context rejected/,
  );
  assert.equal(
    render.messages.some(({ message }) => message.type === "rebuild_engine"),
    false,
    "failed candidate must not touch the live renderer",
  );
  assert.equal(client.mode, AUTHORING_EXECUTION_SEMANTIC);

  authoring.failContext = null;
  const rerun = client.reconcileSemanticExecution(
    { contextId: "semantic-2" },
    { authoringClient: authoring },
  );
  const rebuild = await waitForRequest(render, "rebuild_engine");
  render.emitMessage(
    envelope("noon.render", "engine_rebuilt", {
      requestId: rebuild.requestId,
      mode: "legacy",
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );
  const result = await rerun;
  assert.equal(result.ready.session, 2);
  assert.equal(authoring.attachments.at(-1).options.session, 2);
  await Promise.resolve();
  assert.deepEqual(authoring.stoppedContexts, ["semantic-1"]);
  assert.deepEqual(authoring.releasedContexts, ["semantic-1"]);
  client.terminate();
});

test("semantic to legacy switches to the ordinary renderer and retires the context", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const render = await prepare(client);
  const initial = client.startSemanticExecution(
    { contextId: "semantic-3" },
    { authoringClient: authoring },
  );
  await Promise.resolve();
  replyRender(render, "start_engine", "engine_started", { mode: "legacy" });
  await initial;

  const sceneJson = JSON.stringify({ version: 1, objects: [], tracks: [] });
  const switched = client.reconcileScene(sceneJson);
  const engine = FakeWorker.instances.findLast((worker) => worker.name === "noon-engine");
  assert.ok(engine);
  engine.emitMessage(envelope("noon.engine", "ready", { transportMode: "transferable" }));
  const renderSwitch = await waitForRequest(render, "switch_engine");
  render.emitMessage(
    envelope("noon.render", "mode_switched", {
      requestId: renderSwitch.requestId,
      mode: "legacy",
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );
  const stateRequest = await waitForRequest(engine, "state");
  engine.emitMessage(
    envelope("noon.engine", "state", {
      requestId: stateRequest.requestId,
      time: 0,
      playing: true,
      nextPatchSequence: "0",
      sceneJson,
    }),
  );
  await switched;
  await Promise.resolve();
  assert.equal(client.mode, "legacy");
  assert.equal(render.terminated, false);
  assert.deepEqual(authoring.stoppedContexts, ["semantic-3"]);
  assert.deepEqual(authoring.releasedContexts, ["semantic-3"]);
  assert.equal(FakeWorker.instances.filter(({ name }) => name === "noon-render").length, 1);
  client.terminate();
});

test("legacy to semantic switches to the shared retained renderer", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const sceneJson = JSON.stringify({ version: 1, objects: [], tracks: [] });
  const initial = client.start(sceneJson, { transportMode: "transferable" });
  const engine = FakeWorker.instances.findLast((worker) => worker.name === "noon-engine");
  const render = renderWorker();
  engine.emitMessage(envelope("noon.engine", "ready", { transportMode: "transferable" }));
  render.emitMessage(
    envelope("noon.render", "ready", {
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );
  await initial;

  const switched = client.reconcileSemanticExecution(
    { contextId: "semantic-4" },
    { authoringClient: authoring },
  );
  const renderSwitch = await waitForRequest(render, "switch_engine");
  assert.equal(renderSwitch.mode, "retained");
  render.emitMessage(
    envelope("noon.render", "mode_switched", {
      requestId: renderSwitch.requestId,
      mode: "retained",
      transportMode: "transferable",
      backend: "WebGL2",
      retained: true,
    }),
  );
  await switched;
  assert.equal(client.mode, AUTHORING_EXECUTION_SEMANTIC);
  assert.equal(render.terminated, false);
  assert.equal(engine.terminated, true);
  assert.equal(FakeWorker.instances.filter(({ name }) => name === "noon-render").length, 1);
  client.terminate();
});

test("retained and semantic transitions rebuild only their shared retained renderer", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const sceneJson = JSON.stringify({ version: 1, objects: [], tracks: [] });
  const sceneSpecJson = JSON.stringify({ version: 1, objects: [], tracks: [] });
  const retainedStart = client.startRetainedCanonical(sceneSpecJson, {
    transportMode: "transferable",
  });
  let retainedEngine = FakeWorker.instances.findLast(
    (worker) => worker.name === "noon-mixed-retained-engine",
  );
  const render = renderWorker();
  retainedEngine.emitMessage(
    envelope("noon.engine", "ready", { transportMode: "transferable", retained: true }),
  );
  render.emitMessage(
    envelope("noon.render", "ready", {
      transportMode: "transferable",
      backend: "WebGL2",
      retained: true,
    }),
  );
  await retainedStart;

  const toSemantic = client.reconcileSemanticExecution(
    { contextId: "semantic-5" },
    { authoringClient: authoring },
  );
  let renderRebuild = await waitForRequest(render, "rebuild_engine");
  assert.equal(renderRebuild.mode, "retained");
  render.emitMessage(
    envelope("noon.render", "engine_rebuilt", {
      requestId: renderRebuild.requestId,
      mode: "retained",
      transportMode: "transferable",
      backend: "WebGL2",
      retained: true,
    }),
  );
  await toSemantic;
  assert.equal(client.mode, AUTHORING_EXECUTION_SEMANTIC);

  const backToRetained = client.reconcileScene(sceneJson, { sceneSpecJson });
  retainedEngine = FakeWorker.instances.findLast(
    (worker) => worker.name === "noon-mixed-retained-engine",
  );
  retainedEngine.emitMessage(
    envelope("noon.engine", "ready", { transportMode: "transferable", retained: true }),
  );
  const priorRebuildCount = render.messages.filter(
    ({ message }) => message.type === "rebuild_engine",
  ).length;
  for (;;) {
    const rebuilds = render.messages.filter(({ message }) => message.type === "rebuild_engine");
    if (rebuilds.length > priorRebuildCount) {
      renderRebuild = rebuilds.at(-1).message;
      break;
    }
    await Promise.resolve();
  }
  assert.equal(renderRebuild.mode, "retained");
  render.emitMessage(
    envelope("noon.render", "engine_rebuilt", {
      requestId: renderRebuild.requestId,
      mode: "retained",
      transportMode: "transferable",
      backend: "WebGL2",
      retained: true,
    }),
  );
  const stateRequest = await waitForRequest(retainedEngine, "state");
  retainedEngine.emitMessage(
    envelope("noon.engine", "state", {
      requestId: stateRequest.requestId,
      time: 0,
      playing: true,
      nextPatchSequence: "0",
    }),
  );
  await backToRetained;
  await Promise.resolve();
  assert.equal(client.mode, "retained");
  assert.deepEqual(authoring.stoppedContexts, ["semantic-5"]);
  assert.deepEqual(authoring.releasedContexts, ["semantic-5"]);
  assert.equal(render.terminated, false);
  client.terminate();
});

test("semantic renderer recovery reattaches the same token with a fresh session", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const firstRender = await prepare(client);
  const initial = client.startSemanticExecution(
    { contextId: "semantic-6" },
    { authoringClient: authoring, initiallyPaused: true },
  );
  await Promise.resolve();
  replyRender(firstRender, "start_engine", "engine_started", { mode: "legacy" });
  await initial;

  const restarting = client.restart();
  for (;;) {
    if (FakeWorker.instances.filter(({ name }) => name === "noon-render").length === 2) break;
    await Promise.resolve();
  }
  const secondRender = renderWorker();
  replyRender(secondRender, "prepare", "prepared");
  await waitForRequest(secondRender, "start_engine");
  replyRender(secondRender, "start_engine", "engine_started", { mode: "legacy" });
  const ready = await restarting;

  assert.equal(ready.session, 2);
  assert.deepEqual(
    authoring.attachments.map(({ contextId, options }) => [
      contextId, options.session, options.initiallyPaused,
    ]),
    [
      ["semantic-6", 1, true],
      ["semantic-6", 2, true],
    ],
  );
  assert.deepEqual(authoring.stoppedContexts, ["semantic-6"]);
  assert.deepEqual(authoring.releasedContexts, []);
  assert.equal(firstRender.terminated, true);
  assert.equal(secondRender.terminated, false);
  client.terminate();
});

test("terminating a semantic rerun retires both endpoints without restoring old state", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const render = await prepare(client);
  const initial = client.startSemanticExecution(
    { contextId: "semantic-7" },
    { authoringClient: authoring },
  );
  await Promise.resolve();
  replyRender(render, "start_engine", "engine_started", { mode: "legacy" });
  await initial;

  const transition = client.reconcileSemanticExecution(
    { contextId: "semantic-8" },
    { authoringClient: authoring },
  );
  await waitForRequest(render, "rebuild_engine");
  client.terminate();
  await assert.rejects(transition, /terminated during an asynchronous operation/);
  await Promise.resolve();
  assert.deepEqual(new Set(authoring.stoppedContexts), new Set(["semantic-7", "semantic-8"]));
  assert.deepEqual(authoring.releasedContexts, ["semantic-7"]);
  await assert.rejects(client.state(), /has not been started/);
});

test("terminating semantic to legacy transition retires the hidden old endpoint", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const render = await prepare(client);
  const initial = client.startSemanticExecution(
    { contextId: "semantic-9" },
    { authoringClient: authoring },
  );
  await Promise.resolve();
  replyRender(render, "start_engine", "engine_started", { mode: "legacy" });
  await initial;

  const sceneJson = JSON.stringify({ version: 1, objects: [], tracks: [] });
  const transition = client.reconcileScene(sceneJson);
  const engine = FakeWorker.instances.findLast((worker) => worker.name === "noon-engine");
  engine.emitMessage(envelope("noon.engine", "ready", { transportMode: "transferable" }));
  await waitForRequest(render, "switch_engine");
  client.terminate();
  await assert.rejects(transition, /terminated during an asynchronous operation/);
  await Promise.resolve();
  assert.deepEqual(authoring.stoppedContexts, ["semantic-9"]);
  assert.deepEqual(authoring.releasedContexts, ["semantic-9"]);
  assert.equal(engine.terminated, true);
  await assert.rejects(client.state(), /has not been started/);
});
