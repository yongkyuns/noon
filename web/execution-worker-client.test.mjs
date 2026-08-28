import assert from "node:assert/strict";
import test from "node:test";

class FakeCanvas {
  clientWidth = 640;
  clientHeight = 360;
  width = 640;
  height = 360;
  className = "";
  id = "scene";
  replacement = null;

  transferControlToOffscreen() {
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
    this.#emit("message", { data: message });
  }

  emitError(message = "worker crashed") {
    this.#emit("error", { message });
  }

  emitMessageError() {
    this.#emit("messageerror", {});
  }

  #emit(type, event) {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }
}

globalThis.HTMLCanvasElement = FakeCanvas;
globalThis.Worker = FakeWorker;
globalThis.window = { devicePixelRatio: 1 };

const { ExecutionWorkerClient } = await import("./execution-worker-client.js");

const SCENE_JSON = JSON.stringify({ version: 1, objects: [], tracks: [] });

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

function workerPair(offset = 0) {
  const pair = FakeWorker.instances.slice(offset, offset + 2);
  const engine = pair.find(({ name }) => name === "noon-engine");
  const render = pair.find(({ name }) => name === "noon-render");
  assert.ok(engine, "engine worker must be created");
  assert.ok(render, "render worker must be created");
  return { engine, render };
}

async function startClient(errors = []) {
  const offset = FakeWorker.instances.length;
  const client = new ExecutionWorkerClient(new FakeCanvas(), {
    onError(error, owner) {
      errors.push(`${owner}: ${error.message}`);
    },
  });
  const readyPromise = client.start(SCENE_JSON, { transportMode: "transferable" });
  const { engine, render } = workerPair(offset);
  engine.emitMessage(engineMessage("ready", { transportMode: "transferable" }));
  render.emitMessage(
    renderMessage("ready", { transportMode: "transferable", backend: "WebGL2" }),
  );
  const ready = await readyPromise;
  assert.equal(ready.session, 1);
  return { client, engine, render };
}

function requestMessage(worker, type) {
  const entry = worker.messages.findLast(({ message }) => message.type === type);
  assert.ok(entry, `missing ${worker.name} ${type} request`);
  return entry.message;
}

async function waitForRequest(worker, type, occurrence = 1) {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const entries = worker.messages.filter(({ message }) => message.type === type);
    if (entries.length >= occurrence) {
      return entries[occurrence - 1].message;
    }
    await Promise.resolve();
  }
  assert.fail(`missing ${worker.name} ${type} request`);
}

test("engine and render requests use independent issuance spaces", async () => {
  const errors = [];
  const { client, engine, render } = await startClient(errors);

  const metricsPromise = client.metrics();
  await Promise.resolve();
  const engineMetrics = requestMessage(engine, "metrics");
  const renderMetrics = requestMessage(render, "metrics");
  assert.equal(engineMetrics.requestId, 0);
  assert.equal(renderMetrics.requestId, 0);

  engine.emitMessage(engineMessage("metrics", { requestId: 0, metrics: { host: {} } }));
  render.emitMessage(renderMessage("metrics", { requestId: 0, metrics: { ready: true } }));
  const metrics = await metricsPromise;
  assert.deepEqual(metrics.metrics, { ready: true });
  assert.deepEqual(metrics.engineMetrics, { host: {} });

  assert.deepEqual(client.diagnostics, {
    session: 1,
    engine: {
      nextRequestId: 1,
      pendingRequests: 0,
      staleResponses: 0,
      staleWorkerEvents: 0,
    },
    render: {
      nextRequestId: 1,
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
    nextPatchSequence: "0",
    sceneJson: SCENE_JSON,
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
  assert.equal(renderMetrics.requestId, 0);

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
  const { client, engine: oldEngine, render: oldRender } = await startClient(errors);

  const beforeRestart = client.state();
  await Promise.resolve();
  const firstStateRequest = requestMessage(oldEngine, "state");
  assert.equal(firstStateRequest.requestId, 0);
  oldEngine.emitMessage(
    engineMessage("state", {
      requestId: 0,
      time: 0,
      nextPatchSequence: "0",
      sceneJson: SCENE_JSON,
    }),
  );
  await beforeRestart;

  const offset = FakeWorker.instances.length;
  const restartPromise = client.restart();
  const { engine: newEngine, render: newRender } = workerPair(offset);
  newEngine.emitMessage(engineMessage("ready", { transportMode: "transferable" }));
  newRender.emitMessage(
    renderMessage("ready", { transportMode: "transferable", backend: "WebGL2" }),
  );
  const restarted = await restartPromise;
  assert.equal(restarted.session, 2);
  assert.equal(oldEngine.terminated, true);
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
      nextPatchSequence: "0",
      sceneJson: SCENE_JSON,
    }),
  );
  await statePromise;
  assert.deepEqual(errors, []);
  client.terminate();
});

test("engine failure reconnects without replacing the render worker or canvas", async () => {
  const errors = [];
  const { client, engine: oldEngine, render } = await startClient(errors);
  const canvas = client.canvas;

  oldEngine.emitError("engine lost");
  assert.deepEqual(errors, ["engine: engine lost"]);

  const offset = FakeWorker.instances.length;
  const restartPromise = client.restart();
  await Promise.resolve();
  const newEngine = FakeWorker.instances[offset];
  assert.ok(newEngine, "replacement engine worker must be created");
  assert.equal(newEngine.name, "noon-engine");
  assert.equal(FakeWorker.instances.length, offset + 1, "engine reconnect must not create a render worker");

  const attachRequest = requestMessage(render, "attach_engine");
  render.emitMessage(
    renderMessage("engine_port_attached", {
      requestId: attachRequest.requestId,
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );
  newEngine.emitMessage(engineMessage("ready", { transportMode: "transferable" }));

  const restarted = await restartPromise;
  assert.equal(restarted.session, 2);
  assert.equal(client.canvas, canvas, "engine reconnect must preserve the transferred canvas");
  assert.equal(oldEngine.terminated, true);
  assert.equal(render.terminated, false);

  oldEngine.emitMessage({ malformed: true });
  oldEngine.emitMessageError();
  assert.equal(client.diagnostics.engine.staleWorkerEvents, 2);
  assert.deepEqual(errors, ["engine: engine lost"]);

  const statePromise = client.state();
  await Promise.resolve();
  const stateRequest = requestMessage(newEngine, "state");
  newEngine.emitMessage(
    engineMessage("state", {
      requestId: stateRequest.requestId,
      time: 0,
      nextPatchSequence: "0",
      sceneJson: SCENE_JSON,
    }),
  );
  await statePromise;
  client.terminate();
});

test("engine reconnect restores pause mode and callback phase configuration", async () => {
  const errors = [];
  const { client, engine: oldEngine, render } = await startClient(errors);
  const callbacks = {
    session_id: 7,
    slots: [
      {
        id: 3,
        objects: [0],
        active_after: 0.25,
        active_through: 1.75,
      },
    ],
  };
  const authoringClient = {
    ports: [],
    async attachEnginePort(port) {
      this.ports.push(port);
    },
  };

  const configurePromise = client.configureHostCallbacks(callbacks, authoringClient);
  const initialHostAttach = await waitForRequest(oldEngine, "attach_host_port");
  oldEngine.emitMessage(
    engineMessage("host_port_attached", { requestId: initialHostAttach.requestId }),
  );
  const initialConfigure = await waitForRequest(oldEngine, "configure_callbacks");
  assert.deepEqual(initialConfigure.callbacks, callbacks);
  oldEngine.emitMessage(
    engineMessage("callbacks_configured", {
      requestId: initialConfigure.requestId,
      enabled: true,
      generation: 1,
    }),
  );
  await configurePromise;

  const pausePromise = client.pause();
  const initialPause = await waitForRequest(oldEngine, "pause");
  oldEngine.emitMessage(
    engineMessage("result", {
      requestId: initialPause.requestId,
      operation: "pause",
      time: 0.5,
      playing: false,
      nextPatchSequence: "0",
      sceneJson: SCENE_JSON,
    }),
  );
  await pausePromise;

  oldEngine.emitError("engine lost");
  const canvas = client.canvas;
  const offset = FakeWorker.instances.length;
  const restartPromise = client.restart();
  const newEngine = await waitForNewEngine(offset);

  const renderAttach = await waitForRequest(render, "attach_engine");
  render.emitMessage(
    renderMessage("engine_port_attached", {
      requestId: renderAttach.requestId,
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );
  newEngine.emitMessage(engineMessage("ready", { transportMode: "transferable" }));

  const restoredPause = await waitForRequest(newEngine, "pause");
  newEngine.emitMessage(
    engineMessage("result", {
      requestId: restoredPause.requestId,
      operation: "pause",
      time: 0,
      playing: false,
      nextPatchSequence: "0",
      sceneJson: SCENE_JSON,
    }),
  );

  const restoredHostAttach = await waitForRequest(newEngine, "attach_host_port");
  newEngine.emitMessage(
    engineMessage("host_port_attached", { requestId: restoredHostAttach.requestId }),
  );
  const restoredConfigure = await waitForRequest(newEngine, "configure_callbacks");
  assert.deepEqual(
    restoredConfigure.callbacks,
    callbacks,
    "reconnect must preserve callback activation windows",
  );
  newEngine.emitMessage(
    engineMessage("callbacks_configured", {
      requestId: restoredConfigure.requestId,
      enabled: true,
      generation: 1,
    }),
  );

  const phaseRequest = await waitForRequest(newEngine, "request_callback_phase");
  newEngine.emitMessage(
    engineMessage("callback_phase_requested", {
      requestId: phaseRequest.requestId,
      generation: 1,
      hostRequestId: 0,
    }),
  );

  const restarted = await restartPromise;
  assert.equal(restarted.session, 2);
  assert.equal(client.canvas, canvas);
  assert.equal(render.terminated, false);
  assert.equal(authoringClient.ports.length, 2, "Python host port must be replaced on reconnect");
  assert.deepEqual(errors, ["engine: engine lost"]);
  client.terminate();
});

async function waitForNewEngine(offset) {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const worker = FakeWorker.instances[offset];
    if (worker !== undefined) {
      assert.equal(worker.name, "noon-engine");
      return worker;
    }
    await Promise.resolve();
  }
  assert.fail("replacement engine worker must be created");
}
