import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const controllerSource = await readFile(
  new URL("./authoring-render-controller.js", import.meta.url),
  "utf8",
);
const metricsSource = (await readFile(new URL("./frame-metrics.js", import.meta.url), "utf8"))
  .replace(/^export\s+/gm, "");
const executableSource = metricsSource + "\n" + controllerSource
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

test("retired geometry mode is rejected before canvas or port admission", async () => {
  const messages = [];
  const port = new FakePort();
  const context = vm.createContext({
    port,
    messages,
    MessagePort: FakePort,
    init: () => { throw new Error("invalid mode must not initialize WASM"); },
  });
  vm.runInContext(executableSource, context);
  await vm.runInContext(`
    controller = createAuthoringRenderController({ postMessage: message => messages.push(message) });
    controller.dispatch({ channel: "noon.render", protocolVersion: 1, type: "init", port, mode: "legacy" });
  `, context);
  assert.equal(messages.length, 1);
  assert.equal(messages[0].type, "error");
  assert.match(messages[0].message, /unsupported authoring render mode legacy/);
  assert.equal(port.closed, false);
  assert.equal(port.messages.length, 0);
});

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
      mode:"retained",
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
    renderSubstageProfiling: false,
    renderSubstageSamplesJson: "[]",
    applyDeltaJson: () => true,
    setRendererObservationRequestJson(json) { this.observationRequests.push(json); },
    takeRendererObservationJson() {
      const result = this.observationResult;
      this.observationResult = null;
      return result;
    },
    setRenderSubstageProfiling(enabled) { this.renderSubstageProfiling = enabled; },
    takeRenderSubstageSamplesJson() {
      const result = this.renderSubstageSamplesJson;
      this.renderSubstageSamplesJson = "[]";
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
    objectCount: () => 0,
    lastDrawCalls: () => 0,
    lastInstancesDrawn: () => 0,
    lastBytesUploaded: () => 0,
    lastGeometryCacheMisses: () => 0,
    lastOutlineCacheMisses: () => 0,
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
    RetainedExecutionCanvasRenderer: { create: () => creation.promise },
    SharedExecutionDeltaReader: class { drain() { return 0; } },
    TransferableExecutionDeltaReceiver: class { drain() {} pendingCount() { return 0; } },
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
mode = MODE_RETAINED;
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

test("publication stage metrics are opt-in, exact-publication keyed, and bounded", async () => {
  const harness = await createManagedWakeHarness();
  assert.equal(vm.runInContext("currentMetrics().publicationStageProfiling", harness.context), undefined);
  const request = { channel: "noon.render", protocolVersion: 1, type: "metrics", requestId: 72,
    profilePublicationStages: true };
  await vm.runInContext(`handleMainMessage(${JSON.stringify(request)});`, harness.context);
  const response = harness.mainMessages.find((message) => message.requestId === 72);
  assert.equal(response.metrics.publicationStageProfiling, true);
  assert.deepEqual(JSON.parse(JSON.stringify(response.metrics.publicationStageSamples)), []);
  for (let sequence = 0; sequence < 33; sequence += 1) {
    harness.context.profileJson = JSON.stringify({ sequence });
    vm.runInContext(`consumeDelta(profileJson, {session:53, sequence:${sequence}});`, harness.context);
  }
  const samples = vm.runInContext("currentMetrics().publicationStageSamples", harness.context);
  assert.equal(samples.length, 32);
  assert.deepEqual(JSON.parse(JSON.stringify(samples.map(({ session, sequence }) => [session, sequence]))),
    Array.from({ length: 32 }, (_, index) => [53, index + 1]));
  for (const sample of samples) {
    assert.ok(Number.isFinite(sample.applyMs));
    assert.ok(Number.isFinite(sample.renderMs));
    assert.ok(Number.isFinite(sample.receiveToPresentMs));
    assert.ok(Number.isFinite(sample.ackPostMs));
  }
});

test("render substage timing is opt-in and drains bounded renderer samples", async () => {
  const harness = await createManagedWakeHarness();
  assert.equal(vm.runInContext("currentMetrics().renderSubstageProfiling", harness.context), undefined);
  const first = { session: 53, sequence: 7, encodeCpuWallMs: 1.25 };
  harness.createdRenderer.renderSubstageSamplesJson = JSON.stringify([first]);
  await vm.runInContext(`handleMainMessage({channel:"noon.render", protocolVersion:1,
    type:"metrics", requestId:74, profileRenderSubstages:true});`, harness.context);
  const response = harness.mainMessages.find((message) => message.requestId === 74);
  assert.equal(response.metrics.renderSubstageProfiling, true);
  assert.deepEqual(JSON.parse(JSON.stringify(response.metrics.renderSubstageSamples)), [first]);
  assert.equal(harness.createdRenderer.renderSubstageProfiling, true);
  assert.deepEqual(JSON.parse(JSON.stringify(vm.runInContext(
    "currentMetrics().renderSubstageSamples", harness.context,
  ))), [], "taking metrics drains samples instead of retaining an unbounded history");
});

test("renderer transition discards an old timing sample even when publication identity repeats", async () => {
  const harness = await createManagedWakeHarness();
  await vm.runInContext(`handleMainMessage({
    channel:"noon.render", protocolVersion:1, type:"metrics", requestId:73,
    profilePublicationStages:true,
  });`, harness.context);
  // Model a pending sample when transport/renderer state is reset. Rebuilds may
  // replay the same logical publication identity, so identity matching alone
  // cannot distinguish the old attempt from the new render attempt.
  vm.runInContext(`renderer = null;
    needsPresent = true;
    pendingPresentationPublication = {session:53, sequence:44};
    pendingPublicationStageSample = {session:53, sequence:44, receivedAtMs:0};
    transitionMode = MODE_RETAINED;
    transitionResourceBytes = new Uint8Array([1]);
    transitionFrameLoopWasRunning = false;
    commitRendererTransition("replacement", {session:53, sequence:44});`, harness.context);
  await flushTasks();
  const samples = vm.runInContext("currentMetrics().publicationStageSamples", harness.context);
  assert.deepEqual(JSON.parse(JSON.stringify(samples)), []);
});

test("Rust wake directives admit one animation drive and one deadline without idle polling", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  assert.equal(harness.animationFrames.length, 1);
  harness.animationFrames.shift()(10);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1);
  assert.equal(harness.animationFrames.length, 1, "reserve the next refresh while awaiting the engine response");

  const canceledReservation = harness.animationFrames.shift();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"timer", timerAfterMilliseconds:1000});', harness.context);
  assert.equal(harness.animationFrames.length, 0);
  canceledReservation(900);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1,
    "a browser-snapshotted reservation cannot outrun the authoritative timer wake");
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

test("an animation wake arriving before the queued RAF callback keeps that opportunity", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  assert.equal(harness.animationFrames.length, 1);
  const browserSnapshotCallback = harness.animationFrames[0];

  // Model a message delivered after the browser snapshots RAF callbacks but
  // before it invokes the captured callback. Replacing the RAF loses this
  // presentation opportunity for the current refresh.
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  harness.animationFrames.shift();
  browserSnapshotCallback(16);

  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1);
  assert.equal(harness.nextPort.messages.find((m) => m.type === "tick").timestamp, 16);
  assert.equal(harness.animationFrames.length, 1, "the admitted tick reserves the following refresh");
});

test("a fast animation response reuses a RAF already snapshotted by the browser", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  harness.animationFrames.shift()(8);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1);
  assert.equal(harness.animationFrames.length, 1, "one refresh was reserved after the tick");
  const snapshottedCallback = harness.animationFrames[0];

  // The browser has already captured this callback for its next refresh. A
  // fast worker response must keep that exact opportunity instead of replacing it.
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  harness.animationFrames.shift();
  snapshottedCallback(16);

  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 2);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").at(-1).timestamp, 16);
  assert.equal(harness.animationFrames.length, 1, "the next admitted tick reserves one refresh");
});

test("a reserved RAF survives delta delivery before the next animation wake", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  harness.animationFrames.shift()(8);
  const reservedCallback = harness.animationFrames[0];

  vm.runInContext('consumeDelta("callback publication", {session:14, sequence:1});', harness.context);
  assert.equal(harness.animationFrames.length, 1, "delta presentation preserves the reservation");
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  harness.animationFrames.shift();
  reservedCallback(16);

  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 2);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").at(-1).timestamp, 16);
});

test("a reserved RAF firing during a blocked callback sends no tick and does not poll", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  harness.animationFrames.shift()(8);
  assert.equal(harness.animationFrames.length, 1);

  harness.animationFrames.shift()(16);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1);
  assert.equal(harness.animationFrames.length, 0, "an unanswered tick does not start an RAF polling loop");

  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  assert.equal(harness.animationFrames.length, 1, "the response re-arms the next admitted drive");
  harness.animationFrames.shift()(32);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 2);
});

test("an applied delta reuses a pending RAF opportunity for the engine tick", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  const browserSnapshotCallback = harness.animationFrames[0];

  vm.runInContext('consumeDelta("updated frame", {session:14, sequence:1});', harness.context);
  assert.equal(harness.animationFrames.length, 1);
  browserSnapshotCallback(24);

  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1);
  assert.equal(harness.nextPort.messages.find((m) => m.type === "tick").timestamp, 24);
});

test("a no-op delta leaves the pending RAF opportunity intact", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  const browserSnapshotCallback = harness.animationFrames[0];
  harness.createdRenderer.applyDeltaJson = () => false;

  vm.runInContext('consumeDelta("duplicate frame", {session:14, sequence:1});', harness.context);
  browserSnapshotCallback(32);

  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1);
  assert.equal(harness.nextPort.messages.find((m) => m.type === "tick").timestamp, 32);
});

test("an idle wake retires a queued RAF when no presentation is pending", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  harness.animationFrames.shift()(8);
  const obsoleteCallback = harness.animationFrames[0];

  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"idle"});', harness.context);
  obsoleteCallback(16);

  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1,
    "idle retires the reservation without admitting another drive");
  assert.equal(vm.runInContext("scheduledFrame", harness.context), null,
    "idle leaves no live scheduled frame even though the fake host retains canceled callbacks");
});

test("stop retires an awaiting-response RAF generation", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  harness.animationFrames.shift()(8);
  const obsoleteCallback = harness.animationFrames[0];
  vm.runInContext('stop();', harness.context);
  harness.animationFrames.splice(0, 1);
  obsoleteCallback(16);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1);
  assert.equal(vm.runInContext("scheduledFrame", harness.context), null);
});

test("reconnect retires an awaiting-response RAF generation", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  harness.animationFrames.shift()(8);
  const obsoleteCallback = harness.animationFrames[0];
  const replacement = new FakePort();
  harness.context.replacementPort = replacement;

  harness.animationFrames.splice(0, 1);
  vm.runInContext(`attachEngine({port:replacementPort, transportMode, requestId:9, mode:MODE_RETAINED});
    handleRetainedResources({bytes:new Uint8Array([1])});
    consumeDelta("reconnected", {session:15, sequence:0});`, harness.context);
  const reconnectCallback = harness.animationFrames.at(-1);
  obsoleteCallback(24);
  assert.equal(replacement.messages.filter((m) => m.type === "tick").length, 0,
    "a stale callback from the old frame-loop generation cannot tick the replacement port");
  reconnectCallback(32);
  assert.equal(replacement.messages.filter((m) => m.type === "tick").length, 1);
});

test("a timer wake replaces a queued RAF and honors its refreshed deadline", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  const obsoleteCallback = harness.animationFrames[0];

  harness.setClock(100);
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"timer", timerAfterMilliseconds:50});', harness.context);
  obsoleteCallback(110);
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 0);
  assert.equal(harness.timers.size, 1);
  const [timerId, timer] = [...harness.timers][0];
  assert.equal(timer.delay, 50);

  harness.setClock(149);
  harness.timers.delete(timerId);
  timer.callback();
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 0);
  assert.equal(harness.timers.size, 1, "the still-future deadline is rescheduled");
  const [deadlineId, deadline] = [...harness.timers][0];
  assert.equal(deadline.delay, 1);
  harness.setClock(150);
  harness.timers.delete(deadlineId);
  deadline.callback();
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1);
});

test("animation wake without RAF uses one drive timer and creates no response reservation timer", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('host.requestAnimationFrame = null;', harness.context);
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  assert.equal(harness.timers.size, 1, "the fallback timer admits the requested initial drive");
  const [timerId, timer] = [...harness.timers][0];
  harness.setClock(16);
  harness.timers.delete(timerId);
  timer.callback();
  assert.equal(harness.nextPort.messages.filter((m) => m.type === "tick").length, 1);
  assert.equal(harness.timers.size, 0,
    "without RAF, awaiting the engine response does not reserve a timer polling callback");
});

test("live frame gaps include failed-present stalls but not metrics polls or no-op deltas", async () => {
  const harness = await createManagedWakeHarness([true, true, true, false, true]);
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  const present = (sequence, timestamp) => {
    harness.setClock(timestamp);
    vm.runInContext(`consumeDelta("frame", {session:12, sequence:${sequence}});`, harness.context);
  };
  present(1, 100);
  present(2, 116);
  present(3, 132); // Surface did not present; the elapsed stall still counts.
  assert.equal(vm.runInContext("currentMetrics().presentationIntervalSamples", harness.context), 1);
  harness.setClock(182);
  assert.equal(vm.runInContext("tryPresent()", harness.context), true);
  const summary = vm.runInContext("currentMetrics().presentationIntervalMs", harness.context);
  assert.deepEqual(JSON.parse(JSON.stringify(summary)), {
    min: 16, p50: 16, p95: 66, p99: 66, max: 66, mean: 41,
  });
  harness.setClock(10_000);
  harness.createdRenderer.applyDeltaJson = () => false;
  vm.runInContext('consumeDelta("duplicate", {session:12, sequence:3});', harness.context);
  assert.deepEqual(JSON.parse(JSON.stringify(vm.runInContext(
    "currentMetrics().presentationIntervalMs", harness.context,
  ))), JSON.parse(JSON.stringify(summary)), "observation cannot manufacture presentation gaps");
});

test("continuous frame-gap windows exclude holds and reset on session and surface replacement", async () => {
  const harness = await createManagedWakeHarness();
  const wake = (cadence) => vm.runInContext(
    `handleEngineMessage({type:"execution_wake", cadence:"${cadence}", timerAfterMilliseconds:1000});`,
    harness.context,
  );
  const present = (session, sequence, timestamp) => {
    harness.setClock(timestamp);
    vm.runInContext(`consumeDelta("frame", {session:${session}, sequence:${sequence}});`, harness.context);
  };
  for (const cadence of ["timer", "idle"]) {
    wake("animation_frame");
    present(12, 1, 100); present(12, 2, 116);
    assert.equal(vm.runInContext("currentMetrics().presentationIntervalMs.max", harness.context), 16);
    wake(cadence);
    present(12, 3, 2000);
    assert.equal(vm.runInContext("currentMetrics().presentationIntervalMs", harness.context), null);
    wake("animation_frame");
    present(12, 4, 3000); present(12, 5, 3017);
    assert.equal(vm.runInContext("currentMetrics().presentationIntervalMs.max", harness.context), 17);
    present(13, 0, 4000);
    assert.equal(vm.runInContext("currentMetrics().presentationIntervalMs", harness.context), null);
  }
  present(13, 1, 4016);
  assert.equal(vm.runInContext("currentMetrics().presentationIntervalSamples", harness.context), 1);
  vm.runInContext('suspendForWebGlContextLoss({preventDefault(){}});', harness.context);
  assert.equal(vm.runInContext("currentMetrics().presentationIntervalMs", harness.context), null);
  vm.runInContext("detachRenderPort();", harness.context);
  assert.equal(vm.runInContext("continuousPresentation", harness.context), false);
});

test("live frame-gap collection is bounded and adds no timer or engine drive", async () => {
  const harness = await createManagedWakeHarness();
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"animation_frame"});', harness.context);
  for (let sequence = 0; sequence <= 240; sequence += 1) {
    harness.setClock(sequence === 0 ? 0 : 1000 + sequence * 16);
    vm.runInContext(`consumeDelta("frame", {session:12, sequence:${sequence}});`, harness.context);
  }
  assert.equal(vm.runInContext("currentMetrics().presentationIntervalSamples", harness.context), 120);
  assert.equal(vm.runInContext("currentMetrics().presentationIntervalMs.max", harness.context), 16,
    "a spike older than the rolling window must retire");
  assert.equal(harness.timers.size, 0);
  assert.equal(harness.nextPort.messages.filter(message => message.type === "tick").length, 0);
  // This fake retains cancelled RAF callbacks. Tickets must still admit only
  // the single existing Rust directive when every queued callback is delivered.
  for (const callback of harness.animationFrames.splice(0)) callback(5000);
  assert.equal(harness.nextPort.messages.filter(message => message.type === "tick").length, 1);
  vm.runInContext('handleEngineMessage({type:"execution_wake", cadence:"idle"});', harness.context);
  for (const callback of harness.animationFrames.splice(0)) callback(6000);
  assert.equal(vm.runInContext("scheduledFrame", harness.context), null, "idle stays asleep");
});

test("renderer telemetry timestamps pending snapshots without inventing presentations", () => {
  const harness = createWorkerHarness();
  let now = 1000;
  harness.context.performance = { now: () => now, timeOrigin: 50_000 };
  const before = vm.runInContext("currentMetrics()", harness.context);
  now = 2000;
  const after = vm.runInContext("currentMetrics()", harness.context);
  assert.equal(before.sampledAtMs, 1000);
  assert.equal(after.sampledAtMs, 2000);
  assert.equal(before.performanceTimeOriginMs, 50_000);
  assert.equal(before.firstPresentedAtMs, null);
  assert.equal(after.presentedFrames, before.presentedFrames);
});

test("first presentation timestamp latches only after renderer reports a successful present", () => {
  const harness = createWorkerHarness();
  let now = 100;
  harness.context.performance = { now: () => now, timeOrigin: 70_000 };
  harness.context.shouldRender = false;
  vm.runInContext(`
    renderer = {
      render: () => shouldRender,
      rendererBackend: () => "WebGPU",
      gpuGeneration: () => 1,
      time: () => 0,
      objectCount: () => 1,
      lastDrawCalls: () => 1,
      lastInstancesDrawn: () => 1,
      lastBytesUploaded: () => 0,
      lastGeometryCacheMisses: () => 0,
    };
    mode = null;
    needsPresent = true;
  `, harness.context);

  assert.equal(vm.runInContext("tryPresent()", harness.context), false);
  assert.equal(vm.runInContext("currentMetrics().presentedFrames", harness.context), 0);
  assert.equal(vm.runInContext("currentMetrics().firstPresentedAtMs", harness.context), null);
  assert.equal(vm.runInContext("currentMetrics().lastDeltaApplyMs", harness.context), null);
  assert.equal(vm.runInContext("currentMetrics().lastRendererCallMs", harness.context), null);

  harness.context.advanceClock = () => { now += 5; };
  vm.runInContext("renderer.applyDeltaJson = () => { advanceClock(); return true; }; applyRendererDelta('{}')", harness.context);
  assert.equal(vm.runInContext("currentMetrics().lastDeltaApplyMs", harness.context), 5);
  vm.runInContext("renderer.applyDeltaJson = () => false; applyRendererDelta('{}')", harness.context);
  assert.equal(vm.runInContext("currentMetrics().lastDeltaApplyMs", harness.context), 5,
    "a stale delta cannot replace the last successfully applied interval");

  now = 142;
  harness.context.shouldRender = true;
  assert.equal(vm.runInContext("tryPresent()", harness.context), true);
  const first = vm.runInContext("currentMetrics()", harness.context);
  assert.equal(first.presentedFrames, 1);
  assert.equal(first.firstPresentedAtMs, 142);
  assert.equal(first.lastRendererCallMs, 0);
  assert.equal(first.presentedSession, null);
  assert.equal(first.firstPresentedSessionAtMs, null);
  assert.equal(first.performanceTimeOriginMs, 70_000);

  now = 250;
  harness.context.advanceClock = () => { now += 7; };
  vm.runInContext("renderer.render = () => { advanceClock(); return true; }; needsPresent = true; tryPresent()", harness.context);
  assert.equal(vm.runInContext("currentMetrics().firstPresentedAtMs", harness.context), 142);
  assert.equal(vm.runInContext("currentMetrics().lastRendererCallMs", harness.context), 7);

  vm.runInContext("renderer.render = () => false; needsPresent = true; tryPresent()", harness.context);
  assert.equal(vm.runInContext("currentMetrics().lastRendererCallMs", harness.context), 7,
    "a failed present cannot replace the last successful render interval");
});

test("first successful present is attributed to the exact retained transport session", () => {
  const harness = createWorkerHarness();
  let now = 100;
  harness.context.performance = { now: () => now, timeOrigin: 70_000 };
  harness.context.shouldRender = false;
  vm.runInContext(`
    renderer = {
      render: () => shouldRender,
      rendererBackend: () => "webgl",
      gpuGeneration: () => 1,
      time: () => 0,
      objectCount: () => 1,
      lastDrawCalls: () => 1,
      lastInstancesDrawn: () => 1,
      lastBytesUploaded: () => 0,
      lastGeometryCacheMisses: () => 0,
      lastOutlineCacheMisses: () => 0,
      preloadedGeometryCount: () => 0,
      preloadBytesUploaded: () => 0,
    };
    mode = "retained";
    needsPresent = true;
    pendingPresentationPublication = { session: 7, sequence: 0 };
  `, harness.context);

  assert.equal(vm.runInContext("tryPresent()", harness.context), false);
  assert.equal(vm.runInContext("currentMetrics().presentedSession", harness.context), null);
  assert.equal(vm.runInContext("currentMetrics().firstPresentedSessionAtMs", harness.context), null);
  harness.context.shouldRender = true;
  assert.equal(vm.runInContext("tryPresent()", harness.context), true);
  assert.deepEqual(
    { session: vm.runInContext("currentMetrics().presentedSession", harness.context),
      at: vm.runInContext("currentMetrics().firstPresentedSessionAtMs", harness.context) },
    { session: 7, at: 100 },
  );

  now = 250;
  vm.runInContext(`
    needsPresent = true;
    pendingPresentationPublication = { session: 8, sequence: 0 };
    tryPresent();
  `, harness.context);
  const current = vm.runInContext("currentMetrics()", harness.context);
  assert.equal(current.presentedSession, 8);
  assert.equal(current.firstPresentedSessionAtMs, 250);

  now = 300;
  vm.runInContext(`
    needsPresent = true;
    pendingPresentationPublication = { session: 8, sequence: 1 };
    tryPresent();
  `, harness.context);
  assert.equal(vm.runInContext("currentMetrics().firstPresentedSessionAtMs", harness.context), 250,
    "later publications in the same session must not move its first-present timestamp");
});

test("renderer-ready timestamp records GPU renderer creation before a successful present", async () => {
  const harness = createWorkerHarness([false]);
  let now = 125;
  harness.context.performance = { now: () => now, timeOrigin: 70_000 };
  Object.assign(harness.createdRenderer, {
    objectCount: () => 1,
    lastDrawCalls: () => 1,
    lastInstancesDrawn: () => 1,
    lastBytesUploaded: () => 0,
    lastGeometryCacheMisses: () => 0,
    lastOutlineCacheMisses: () => 0,
  });
  harness.creation.resolve(harness.createdRenderer);
  await flushTasks();

  const pending = vm.runInContext("currentMetrics()", harness.context);
  assert.equal(pending.rendererReadyAtMs, 125);
  assert.equal(pending.firstPresentedAtMs, null);
  assert.equal(pending.performanceTimeOriginMs, 70_000);
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
  assert.equal(harness.animationFrames.length, 1,
    "the admitted tick reserves one refresh while its response is pending");
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


test("clamped zero-to-one resize stays idle and later real resizes still present", () => {
  const sizes = [];
  let presents = 0;
  const context = vm.createContext({
    performance: { now: () => 0, timeOrigin: 0 },
    rendererStub: {
      resize: (width, height) => sizes.push([width, height]),
      render: () => { presents += 1; return true; },
    },
    drainRendererGpuDiagnostics: () => true,
    formatGpuDiagnostic: String,
  });
  vm.runInContext(harnessSource, context);
  vm.runInContext(`renderer = rendererStub; width = 640; height = 360;
    resize({width:0, height:0});`, context);
  assert.equal(presents, 1);
  vm.runInContext("resize({width:1, height:1});", context);
  assert.equal(presents, 1, "equivalent 1px backing size must not force another frame");
  assert.equal(vm.runInContext("needsPresent", context), false);
  assert.equal(vm.runInContext("scheduledFrame", context), null);
  vm.runInContext("resize({width:2, height:2}); resize({width:640, height:360});", context);
  assert.equal(presents, 3, "changed backing dimensions must remain drawable after the no-op");
  assert.deepEqual(sizes, [[1,1], [1,1], [2,2], [640,360]]);
  assert.equal(vm.runInContext("needsPresent", context), false);
  assert.equal(vm.runInContext("scheduledFrame", context), null);
});

test("selection-only publications retry presentation, gate clear, and settle without an engine tick", async () => {
  const harness = await createManagedWakeHarness([true, false, true, true]);
  const applied = [];
  // GPU results are deliberately stubbed: this proves production controller
  // ordering/acknowledgement, not the shape or pixels drawn by Rust/WASM.
  harness.createdRenderer.applyDeltaJson = json => { applied.push(JSON.parse(json)); return true; };
  const selected = { channel: "noon.execution.retained", protocol_version: 16,
    session: 12, sequence: 4, time: 0, snapshot: false, objects: [],
    selection_overlay: { geometry: { kind: "circle", radius: 1 },
      transform: { translation: {x:0,y:0}, scale: {x:1,y:1}, rotation: 0 } } };
  const cleared = { ...selected, sequence: 5 };
  delete cleared.selection_overlay;
  harness.context.selectedJson = JSON.stringify(selected);
  harness.context.clearedJson = JSON.stringify(cleared);
  assert.equal(vm.runInContext('consumeDelta(selectedJson, {session:12, sequence:4});', harness.context), true);
  assert.equal(harness.nextPort.messages.filter(m => m.type === "execution_presented").length, 0);
  assert.equal(vm.runInContext('consumeDelta(clearedJson, {session:12, sequence:5});', harness.context), false);
  assert.deepEqual(applied, [selected]);
  assert.equal(harness.animationFrames.length, 1);
  harness.animationFrames.shift()(10);
  assert.deepEqual(harness.nextPort.messages.filter(m => m.type === "execution_presented").map(m => m.sequence), [4]);
  assert.equal(vm.runInContext('consumeDelta(clearedJson, {session:12, sequence:5});', harness.context), true);
  assert.deepEqual(applied, [selected, cleared]);
  assert.deepEqual(harness.nextPort.messages.filter(m => m.type === "execution_presented").map(m => m.sequence), [4, 5]);
  assert.equal(harness.nextPort.messages.filter(m => m.type === "tick").length, 0);
  assert.equal(harness.animationFrames.length, 0);
  assert.equal(harness.timers.size, 0);
});

test("worker receipt names the logical view only after successful render and never from consumption", async () => {
  const harness = createWorkerHarness([true, false, true]);
  harness.creation.resolve(harness.createdRenderer); await flushTasks();
  harness.animationFrames.shift()(10);
  vm.runInContext('consumeDelta("with-view", {session:12,sequence:4,pointerView:{revision:3,width:800,height:400}});', harness.context);
  assert.equal(harness.nextPort.messages.filter(m => m.pointerReceipt).length,0);
  harness.animationFrames.shift()(20); await flushTasks();
  const ack = harness.nextPort.messages.find(m => m.pointerReceipt);
  assert.ok(ack, "successful render must acknowledge its pointer view");
  assert.deepEqual(JSON.parse(JSON.stringify(ack.pointerReceipt)),
    {session:12,sequence:4,presentation:2,view_revision:3});
  const count=harness.createdRenderer.renderCalls;
  harness.createdRenderer.applyDeltaJson=()=>false;
  vm.runInContext('consumeDelta("duplicate", {session:12,sequence:4,pointerView:{revision:3,width:800,height:400}});',harness.context);
  assert.equal(harness.createdRenderer.renderCalls,count);
  const repeated=harness.nextPort.messages.filter(m=>m.pointerReceipt).at(-1);
  assert.deepEqual(repeated.pointerReceipt,ack.pointerReceipt,"duplicate receipt cannot invent a new presentation");
});

test("surface invalidation retires a receipt once and a repaint gets a newer presentation identity", async () => {
  const harness=createWorkerHarness([true,true,true]);
  harness.creation.resolve(harness.createdRenderer);await flushTasks();
  harness.animationFrames.shift()(10);harness.animationFrames.shift()(20);await flushTasks();
  vm.runInContext('consumeDelta("with-view", {session:12,sequence:4,pointerView:{revision:3,width:800,height:400}});',harness.context);
  const a=harness.nextPort.messages.findLast(m=>m.pointerReceipt).pointerReceipt;
  vm.runInContext('invalidatePointerReceipt(); invalidatePointerReceipt(); needsPresent=true; tryPresent();',harness.context);
  const invalid=harness.nextPort.messages.filter(m=>m.type==="pointer_presentation_invalidated");
  assert.equal(invalid.length,1);assert.deepEqual(invalid[0].receipt,a);
  const b=harness.nextPort.messages.findLast(m=>m.pointerReceipt).pointerReceipt;
  assert.equal(b.sequence,a.sequence);assert.ok(b.presentation>a.presentation);
});

test("WebGL loss holds publications and resize until recovery presents the pending frame", async () => {
  const harness = await createManagedWakeHarness([true, false, true]);
  const recovery = deferred();
  const sizes = [];
  harness.createdRenderer.recoverWebGlContext = () => recovery.promise;
  harness.createdRenderer.resize = (width, height) => sizes.push([width, height]);
  vm.runInContext('consumeDelta("pending", {session:12, sequence:4});', harness.context);
  const before = harness.createdRenderer.renderCalls;
  let prevented = false;
  harness.context.lossEvent = { preventDefault() { prevented = true; } };
  vm.runInContext('suspendForWebGlContextLoss(lossEvent); resize({width:390, height:844});', harness.context);
  assert.equal(prevented, true);
  assert.equal(vm.runInContext('consumeDelta("next", {session:12, sequence:5})', harness.context), false);
  assert.equal(vm.runInContext('tryPresent()', harness.context), false);
  for (const callback of harness.animationFrames.splice(0)) callback(10);
  assert.equal(harness.createdRenderer.renderCalls, before);
  assert.deepEqual(sizes, []);
  assert.equal(harness.nextPort.messages.filter(m => m.type === "tick").length, 0);
  assert.equal(harness.nextPort.messages.filter(m => m.type === "execution_presented").length, 0);

  const restoring = vm.runInContext('recoverAndPresentWebGlContext()', harness.context);
  await flushTasks();
  assert.equal(vm.runInContext('consumeDelta("next", {session:12, sequence:5})', harness.context), false);
  recovery.resolve(true);
  await restoring;
  assert.deepEqual(sizes, [[390, 844]]);
  assert.deepEqual(harness.nextPort.messages.filter(m => m.type === "execution_presented").map(m => m.sequence), [4]);
  assert.equal(vm.runInContext('webglContextLost', harness.context), false);
  assert.equal(harness.animationFrames.length, 0, "recovered idle engine must not poll");
});

test("WebGL loss backpressures replay handoff before retiring the current renderer", async () => {
  const harness = await createManagedWakeHarness();
  const replacementPort = new FakePort();
  harness.context.replacementPort = replacementPort;
  vm.runInContext(`
    suspendForWebGlContextLoss({preventDefault() {}});
    beginRendererTransition(
      {port:replacementPort, transportMode, requestId:8}, MODE_RETAINED, "renderer_rebuilt",
    );
    handleRetainedResources({bytes:new Uint8Array([2])});
  `, harness.context);
  assert.equal(vm.runInContext('consumeDelta("replay-snapshot")', harness.context), false);
  assert.equal(harness.createdRenderer.freed, false);
  assert.equal(vm.runInContext('transitionMode', harness.context), "retained");
  assert.equal(vm.runInContext('bootstrapPromise', harness.context), null);
});

test("shutdown during WebGL recovery releases the renderer only after its WASM borrow ends", async () => {
  const harness = await createManagedWakeHarness();
  const recovery = deferred();
  harness.createdRenderer.recoverWebGlContext = () => recovery.promise;
  vm.runInContext('suspendForWebGlContextLoss({preventDefault() {}});', harness.context);
  const restoring = vm.runInContext('recoverAndPresentWebGlContext()', harness.context);
  await flushTasks();
  const presents = harness.createdRenderer.renderCalls;
  vm.runInContext('stop()', harness.context);
  assert.equal(harness.createdRenderer.freed, false);
  const messagesBefore = harness.mainMessages.length;
  recovery.reject(new Error("retired context"));
  await restoring;
  assert.equal(harness.createdRenderer.freed, true);
  assert.equal(harness.mainMessages.length, messagesBefore, "retired recovery must not report a new error");
  assert.equal(harness.createdRenderer.renderCalls, presents);
});

test("an incomplete WebGL recovery keeps the renderer suspended", async () => {
  const harness = await createManagedWakeHarness();
  harness.createdRenderer.recoverWebGlContext = async () => false;
  const presents = harness.createdRenderer.renderCalls;
  vm.runInContext('suspendForWebGlContextLoss({preventDefault() {}});', harness.context);
  await vm.runInContext('recoverAndPresentWebGlContext()', harness.context);
  assert.equal(vm.runInContext('webglContextLost', harness.context), true);
  assert.equal(vm.runInContext('consumeDelta("next")', harness.context), false);
  assert.equal(harness.createdRenderer.renderCalls, presents);
  harness.createdRenderer.recoverWebGlContext = async () => true;
  await vm.runInContext('recoverAndPresentWebGlContext()', harness.context);
  assert.equal(vm.runInContext('webglContextLost', harness.context), false);
  assert.equal(harness.createdRenderer.renderCalls, presents + 1);
});

test("WebGL recovery waits for later restoration listeners, beyond a microtask checkpoint", async () => {
  const harness = await createManagedWakeHarness();
  let restorationRecorded = false;
  let recoveries = 0;
  harness.createdRenderer.recoverWebGlContext = async () => {
    assert.equal(restorationRecorded, true, "Rust restoration listener has not run yet");
    recoveries += 1;
    return true;
  };
  harness.context.queueMicrotask = queueMicrotask;
  vm.runInContext(`
    suspendForWebGlContextLoss({preventDefault() {}});
    wakeAfterWebGlContextRestored();
  `, harness.context);
  await flushTasks();
  assert.equal(recoveries, 0, "recovery must not run at an inter-listener microtask checkpoint");
  restorationRecorded = true;
  assert.equal(harness.timers.size, 1);
  const [timerId, timer] = [...harness.timers][0];
  harness.timers.delete(timerId);
  timer.callback();
  await flushTasks();
  assert.equal(recoveries, 1);
  assert.equal(vm.runInContext('webglContextLost', harness.context), false);
});
