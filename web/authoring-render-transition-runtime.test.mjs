import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const controllerSource = await readFile(
  new URL("./authoring-render-controller.js", import.meta.url),
  "utf8",
);
const executableSource = controllerSource
  .replace(/^import\s+[\s\S]*?;\n/gm, "")
  .replace(/^export\s+/gm, "");
const adapterSource = await readFile(
  new URL("./main-thread-render-worker.js", import.meta.url),
  "utf8",
);
const executableAdapterSource = adapterSource
  .replace(/^export\s+/gm, "")
  .replace('import("./authoring-render-controller.js")', "loadControllerModule()");

function unwrapControllerFactory(source) {
  const factoryStart = source.indexOf("function createAuthoringRenderController(host) {");
  const bodyStart = source.indexOf("{", factoryStart) + 1;
  const bodyEnd = source.lastIndexOf("\n}");
  let body = source.slice(bodyStart, bodyEnd);
  body = body.slice(body.indexOf("let renderPort = null;"));
  const controllerReturnStart = body.indexOf("return Object.freeze({");
  const controllerReturnEnd = body.indexOf("});", controllerReturnStart) + 3;
  body = body.slice(0, controllerReturnStart) + body.slice(controllerReturnEnd);
  return `${source.slice(0, factoryStart)}let host = null;\n${body.replace(/^  /gm, "")}`;
}

const harnessSource = unwrapControllerFactory(executableSource);

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((accept, decline) => {
    resolve = accept;
    reject = decline;
  });
  return { promise, resolve, reject };
}

function flushTasks() {
  return new Promise((resolve) => setImmediate(resolve));
}

test("controller instances isolate dispatch and shutdown", async () => {
  const firstMessages = [];
  const secondMessages = [];
  const closed = [];
  const context = vm.createContext({ firstMessages, secondMessages, closed });
  vm.runInContext(executableSource, context);
  vm.runInContext(
    `first = createAuthoringRenderController({
      postMessage: (message) => firstMessages.push(message),
      close: () => closed.push("first"),
    });
    second = createAuthoringRenderController({
      postMessage: (message) => secondMessages.push(message),
      close: () => closed.push("second"),
    });`,
    context,
  );

  await vm.runInContext(
    `first.dispatch({channel:"noon.render", protocolVersion:1, type:"unknown", requestId:11})`,
    context,
  );
  vm.runInContext("first.shutdown()", context);
  await vm.runInContext(
    `second.dispatch({channel:"noon.render", protocolVersion:1, type:"unknown", requestId:22})`,
    context,
  );

  assert.deepEqual(firstMessages.map(({ requestId }) => requestId), [11]);
  assert.deepEqual(secondMessages.map(({ requestId }) => requestId), [22]);
  assert.deepEqual(closed, ["first"]);
});

test("a terminated main-thread adapter cannot shut down a later adapter", async () => {
  const moduleLoad = deferred();
  const messages = [];
  const context = vm.createContext({
    EventTarget,
    MessageEvent,
    queueMicrotask,
    loadControllerModule: () => moduleLoad.promise,
    messages,
  });
  vm.runInContext(executableSource, context);
  vm.runInContext(executableAdapterSource, context);
  vm.runInContext(
    `controllerModule = { createAuthoringRenderController };
    abandoned = new MainThreadRenderWorker();
    abandoned.terminate();
    active = new MainThreadRenderWorker();
    active.addEventListener("message", (event) => messages.push(event.data));
    active.postMessage({
      channel:"noon.render", protocolVersion:1, type:"unknown", requestId:42,
    });`,
    context,
  );
  moduleLoad.resolve(context.controllerModule);
  await flushTasks();

  assert.deepEqual(JSON.parse(JSON.stringify(messages)), [{
    channel: "noon.render",
    protocolVersion: 1,
    type: "error",
    requestId: 42,
    message: "unknown authoring render command unknown",
  }]);
  vm.runInContext("active.terminate()", context);
});

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

test("shutdown during asynchronous initialization cannot revive a controller", async () => {
  const initialization = deferred();
  const closed = [];
  class FakeCanvas {
    width = 10;
    height = 10;
    listeners = 0;
    addEventListener() { this.listeners += 1; }
    removeEventListener() { this.listeners = Math.max(0, this.listeners - 1); }
  }
  const canvas = new FakeCanvas();
  const port = new FakePort();
  const context = vm.createContext({
    canvas,
    port,
    closed,
    init: () => initialization.promise,
    OffscreenCanvas: FakeCanvas,
    MessagePort: FakePort,
    EXECUTION_TRANSPORT_SHARED: "shared",
    EXECUTION_TRANSPORT_TRANSFERABLE: "transferable",
  });
  vm.runInContext(executableSource, context);
  vm.runInContext(
    `controller = createAuthoringRenderController({
      postMessage: () => {},
      close: () => closed.push("closed"),
    });
    initialization = controller.dispatch({
      channel:"noon.render",
      protocolVersion:1,
      type:"init",
      port,
      canvas,
      transportMode:"transferable",
      mode:"legacy",
    });`,
    context,
  );
  vm.runInContext("controller.shutdown()", context);
  initialization.resolve();
  await vm.runInContext("initialization", context);

  assert.equal(canvas.listeners, 0);
  assert.equal(port.closed, false, "an unadmitted port remains owned by its caller");
  assert.deepEqual(closed, ["closed"]);
});

test("concurrent controllers share wasm initialization without sharing lifecycle", async () => {
  const initialization = deferred();
  const firstMessages = [];
  const secondMessages = [];
  let initializationCalls = 0;
  class FakeCanvas {
    width = 10;
    height = 10;
    addEventListener() {}
    removeEventListener() {}
  }
  const context = vm.createContext({
    firstCanvas: new FakeCanvas(),
    secondCanvas: new FakeCanvas(),
    firstMessages,
    secondMessages,
    init: () => {
      initializationCalls += 1;
      return initialization.promise;
    },
    OffscreenCanvas: FakeCanvas,
    EXECUTION_TRANSPORT_SHARED: "shared",
    EXECUTION_TRANSPORT_TRANSFERABLE: "transferable",
  });
  vm.runInContext(executableSource, context);
  vm.runInContext(
    `first = createAuthoringRenderController({
      postMessage: (message) => firstMessages.push(message),
      close: () => {},
    });
    second = createAuthoringRenderController({
      postMessage: (message) => secondMessages.push(message),
      close: () => {},
    });
    firstPreparation = first.dispatch({
      channel:"noon.render", protocolVersion:1, type:"prepare", requestId:1,
      canvas:firstCanvas, transportMode:"transferable",
    });
    secondPreparation = second.dispatch({
      channel:"noon.render", protocolVersion:1, type:"prepare", requestId:2,
      canvas:secondCanvas, transportMode:"transferable",
    });`,
    context,
  );
  await Promise.resolve();
  vm.runInContext("first.shutdown()", context);
  initialization.resolve();
  await vm.runInContext("Promise.all([firstPreparation, secondPreparation])", context);

  assert.equal(initializationCalls, 1);
  assert.deepEqual(firstMessages, []);
  assert.deepEqual(secondMessages.map(({ type, requestId }) => ({ type, requestId })), [
    { type: "prepared", requestId: 2 },
  ]);
});

test("failed shared wasm initialization can be retried by a new controller", async () => {
  const messages = [];
  let initializationCalls = 0;
  class FakeCanvas {
    width = 10;
    height = 10;
    addEventListener() {}
    removeEventListener() {}
  }
  const context = vm.createContext({
    firstCanvas: new FakeCanvas(),
    secondCanvas: new FakeCanvas(),
    messages,
    init: () => {
      initializationCalls += 1;
      return initializationCalls === 1
        ? Promise.reject(new Error("initialization failed"))
        : Promise.resolve();
    },
    OffscreenCanvas: FakeCanvas,
    EXECUTION_TRANSPORT_SHARED: "shared",
    EXECUTION_TRANSPORT_TRANSFERABLE: "transferable",
  });
  vm.runInContext(executableSource, context);
  await vm.runInContext(
    `createAuthoringRenderController({postMessage:(message) => messages.push(message)})
      .dispatch({channel:"noon.render", protocolVersion:1, type:"prepare", requestId:1,
        canvas:firstCanvas, transportMode:"transferable"})`,
    context,
  );
  await vm.runInContext(
    `createAuthoringRenderController({postMessage:(message) => messages.push(message)})
      .dispatch({channel:"noon.render", protocolVersion:1, type:"prepare", requestId:2,
        canvas:secondCanvas, transportMode:"transferable"})`,
    context,
  );

  assert.equal(initializationCalls, 2);
  assert.deepEqual(messages.map(({ type, requestId }) => ({ type, requestId })), [
    { type: "error", requestId: 1 },
    { type: "prepared", requestId: 2 },
  ]);
});

function createRenderer(renderResults) {
  return {
    freed: false,
    renderCalls: 0,
    observationRequests: [],
    observationResult: null,
    applyDeltaJson: () => true,
    setRendererObservationRequestJson(json) { this.observationRequests.push(json); },
    takeRendererObservationJson() {
      const result = this.observationResult;
      this.observationResult = null;
      return result;
    },
    resize() {},
    render() {
      this.renderCalls += 1;
      return renderResults.shift() ?? true;
    },
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
      close() {},
      postMessage(message) { mainMessages.push(message); },
      requestAnimationFrame(callback) { animationFrames.push(callback); },
    },
  });
  vm.runInContext(harnessSource, context);
  vm.runInContext(
    `host = {
      postMessage: (message) => self.postMessage(message),
      close: () => self.close(),
      requestAnimationFrame: (callback) => self.requestAnimationFrame(callback),
    };`,
    context,
  );
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

test("retained transport acknowledges an exact publication only after it presents", async () => {
  const harness = createWorkerHarness([true]);
  harness.creation.resolve(harness.createdRenderer);
  await flushTasks();
  // The stale callback belongs to the retired renderer transition; the current
  // callback presents the bootstrap snapshot and installs the retained port.
  harness.animationFrames.shift()(10);
  harness.animationFrames.shift()(20);
  await flushTasks();

  vm.runInContext(
    'consumeDelta("incremental", { session: 11, sequence: 9 });',
    harness.context,
  );
  const acknowledgement = harness.nextPort.messages.find(
    (message) => message.type === "execution_presented",
  );
  assert.equal(acknowledgement.session, 11);
  assert.equal(acknowledgement.sequence, 9);
});

test("a stale no-op cannot acknowledge ahead of a pending present", async () => {
  const harness = createWorkerHarness([true, false, true]);
  harness.creation.resolve(harness.createdRenderer);
  await flushTasks();
  // Retire the old frame callback and leave the current render loop available
  // to retry the failed incremental present below.
  harness.animationFrames.shift()(10);

  vm.runInContext(
    'consumeDelta("first", { session: 12, sequence: 4 });',
    harness.context,
  );
  vm.runInContext(
    'consumeDelta("stale", { session: 12, sequence: 4 });',
    harness.context,
  );
  assert.equal(
    harness.nextPort.messages.filter((message) => message.type === "execution_presented").length,
    0,
    "a no-op must not acknowledge before the pending frame reaches the surface",
  );

  harness.animationFrames.shift()(20);
  await flushTasks();
  assert.equal(
    harness.nextPort.messages.filter((message) => message.type === "execution_presented").length,
    1,
  );
  const renderCalls = harness.createdRenderer.renderCalls;
  harness.createdRenderer.applyDeltaJson = () => false;
  vm.runInContext(
    'consumeDelta("already-presented", { session: 12, sequence: 4 });',
    harness.context,
  );
  assert.equal(harness.createdRenderer.renderCalls, renderCalls, "stale duplicate must not redraw");
  assert.equal(
    harness.nextPort.messages.filter((message) => message.type === "execution_presented").length,
    2,
    "the exact already-presented publication may acknowledge without a redraw",
  );
});

async function createManagedWakeHarness(renderResults = [true]) {
  const harness = createWorkerHarness(renderResults);
  harness.creation.resolve(harness.createdRenderer);
  await flushTasks();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"idle"});', harness.context);
  for (const callback of harness.animationFrames.splice(0)) callback(0);
  let clock = 0;
  let timerId = 0;
  const timers = new Map();
  harness.context.performance = { now: () => clock };
  harness.context.setTimeout = (callback, delay) => {
    const id = ++timerId;
    timers.set(id, { callback, delay });
    return id;
  };
  harness.context.clearTimeout = (id) => timers.delete(id);
  return { ...harness, timers, setClock(value) { clock = value; } };
}

test("Rust wake directives admit one animation drive and one deadline without idle polling", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  assert.equal(harness.animationFrames.length, 1);
  harness.animationFrames.shift()(10);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1);
  assert.equal(harness.animationFrames.length, 0, "next engine response owns the next wake");

  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"timer", timerAfterMilliseconds:1000});', harness.context);
  assert.equal(harness.animationFrames.length, 0);
  assert.equal(harness.timers.size, 1);
  const [timerId, timer] = [...harness.timers][0];
  assert.equal(timer.delay, 1000);
  harness.setClock(1000);
  harness.timers.delete(timerId);
  timer.callback();
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 2);
  assert.equal(harness.timers.size, 0);
  assert.equal(harness.animationFrames.length, 0);

  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"idle"});', harness.context);
  assert.equal(harness.timers.size, 0);
  assert.equal(harness.animationFrames.length, 0);
});

test("idle continuation retries a pending surface publication without advancing the engine", async () => {
  const harness = await createManagedWakeHarness([true, false, true]);
  vm.runInContext('consumeDelta("endpoint", {session:12, sequence:4});', harness.context);
  assert.equal(harness.animationFrames.length, 1, "transient surface failure requests a draw retry");
  harness.animationFrames.shift()(10);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "execution_presented").length, 1);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 0);
  assert.equal(harness.animationFrames.length, 0);
});

test("paused semantic wake presents exact samples without leaving an animation-frame poll", async () => {
  const harness = await createManagedWakeHarness([true, true]);
  assert.equal(vm.runInContext("presentedFrames", harness.context), 1);
  assert.equal(harness.animationFrames.length, 0, "the initial dirty frame settles to idle");
  assert.equal(harness.nextPort.messages.filter((message) => message.type === "tick").length, 0);

  vm.runInContext(
    'consumeDelta("exact-paused-sample", {session:14, sequence:3});',
    harness.context,
  );
  assert.equal(vm.runInContext("presentedFrames", harness.context), 2);
  assert.equal(
    harness.nextPort.messages.filter((message) => message.type === "execution_presented").at(-1)
      .sequence,
    3,
  );
  assert.equal(harness.animationFrames.length, 0, "an exact paused sample does not restart RAF");
  assert.equal(harness.nextPort.messages.filter((message) => message.type === "tick").length, 0);

  vm.runInContext(
    'handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});',
    harness.context,
  );
  assert.equal(harness.animationFrames.length, 1, "resume cadence schedules one engine drive");
  harness.animationFrames.shift()(25);
  assert.equal(harness.nextPort.messages.filter((message) => message.type === "tick").length, 1);
  assert.equal(
    harness.animationFrames.length,
    0,
    "the engine response must authorize any later animation-frame drive",
  );
});

test("replacement cancels an obsolete continuation deadline before it can tick the next engine", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"timer", timerAfterMilliseconds:1000});', harness.context);
  const timer = [...harness.timers.values()][0];
  vm.runInContext('detachRenderPort();', harness.context);
  assert.equal(harness.timers.size, 0);
  harness.setClock(1000);
  timer.callback();
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 0);
});


test("reconnecting a non-wake engine restores one frame request on its replacement port", async () => {
  const harness = await createManagedWakeHarness();
  const replacement = new FakePort();
  harness.context.replacementPort = replacement;
  vm.runInContext(`
    attachEngine({port:replacementPort, transportMode, requestId:8, mode:MODE_RETAINED});
    handleRetainedResources({bytes:new Uint8Array([1])});
    consumeDelta("reconnected", {session:13, sequence:0});
  `, harness.context);
  assert.equal(harness.animationFrames.length, 1);
  harness.animationFrames.shift()(10);
  assert.equal(replacement.messages.filter((m) => m.type === "tick").length, 1);
  assert.equal(harness.animationFrames.length, 1);
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"idle"});', harness.context);
  for (const callback of harness.animationFrames.splice(0)) callback(20);
  assert.equal(replacement.messages.filter((m) => m.type === "tick").length, 1);
  assert.equal(harness.animationFrames.length, 0);
});

test("retained renderer forwards one observation only after its matching presentation", async () => {
  const harness = createWorkerHarness([true, false, true]);
  harness.creation.resolve(harness.createdRenderer);
  await flushTasks();
  harness.animationFrames.shift()(10);

  const publication = { session: 15, sequence: 6 };
  const request = {
    schema_version: 1,
    publication,
    slot: { slot: 4, generation: 2 },
    committed: {},
  };
  const result = {
    outcome: "presented",
    publication,
    presentation: {
      presentation_sequence: 2,
      submit_called: true,
      present_called: true,
    },
  };
  harness.createdRenderer.observationResult = JSON.stringify(result);
  harness.context.observationRequest = {
    type: "renderer_observation_request",
    ...publication,
    json: JSON.stringify(request),
  };
  harness.context.observationPublication = publication;
  vm.runInContext(
    "handleEngineMessage(observationRequest); consumeDelta('observed', observationPublication);",
    harness.context,
  );

  assert.deepEqual(
    harness.createdRenderer.observationRequests.map(JSON.parse),
    [request],
  );
  assert.equal(
    harness.nextPort.messages.some((message) => message.type === "renderer_observation"),
    false,
    "a failed surface attempt cannot acknowledge prepared/uploaded state as presented",
  );
  harness.animationFrames.shift()(20);
  await flushTasks();
  const messages = harness.nextPort.messages.filter((message) =>
    message.type === "renderer_observation" || message.type === "execution_presented");
  assert.deepEqual(
    messages.map(({ type, session, sequence }) => ({ type, session, sequence })),
    [
      { type: "renderer_observation", ...publication },
      { type: "execution_presented", ...publication },
    ],
  );
  assert.deepEqual(JSON.parse(messages[0].json), result);
});


test("renderer startup reports browser surface creation diagnostics", async () => {
  const harness = createWorkerHarness();
  vm.runInContext('recordSurfaceCreationError({ statusMessage: "GPU surface unavailable" })', harness.context);
  harness.creation.reject(new Error("renderer initialization failed"));
  await flushTasks();
  assert.ok(harness.mainMessages.some((message) =>
    message.type === "error" && message.message.includes("GPU surface unavailable")));
});
