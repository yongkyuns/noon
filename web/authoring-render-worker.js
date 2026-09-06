import init, {
  ExecutionCanvasRenderer,
  RetainedExecutionCanvasRenderer,
} from "./pkg/noon_web.js";
import {
  drainRendererGpuDiagnostics,
  formatGpuDiagnostic,
} from "./render-gpu-diagnostics.js";
import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  SharedExecutionDeltaReader,
  TransferableExecutionDeltaReceiver,
} from "./execution-transport.js";

const RENDER_CHANNEL = "noon.render";
const RENDER_PROTOCOL_VERSION = 1;
const MODE_LEGACY = "legacy";
const MODE_RETAINED = "retained";
const BOOTSTRAP_QUEUE_LIMIT = 1;

let renderPort = null;
let transportMode = null;
let mode = null;
let sharedReader = null;
let transferableReceiver = null;
let renderer = null;
let resourceBytes = null;
let reconnectResourceBundlePending = false;
let canvas = null;
let width = 1;
let height = 1;
let bootstrapQueue = [];
let bootstrapPromise = null;
let transitionRequestId = null;
let transitionResponseType = null;
let transitionMode = null;
let transitionResourceBytes = null;
let transitionFrameLoopWasRunning = false;
let needsPresent = false;
// Renderer-derived metadata for the publication that the next successful
// present exposes. It is an acknowledgement only, never scene or time state.
let pendingPresentationPublication = null;
let lastPresentedPublication = null;
let pendingRendererObservationRequest = null;
let pendingRendererObservationPublication = null;
let running = false;
let stopped = false;
let frameLoopGeneration = 0;
let lastFrameTimestamp = null;
// Optional cross-worker projection of Rust's wake decision. This owns browser
// timer handles only; scene time and segment completion remain in the engine.
let engineWake = null;
let scheduledFrame = null;
let scheduleTicket = 0;
let presentedFrames = 0;
let modeSwitches = 0;
let rendererRebuilds = 0;
let webglRecoveryPromise = null;

self.addEventListener("message", (event) => {
  void handleMainMessage(event.data);
});

async function handleMainMessage(message) {
  try {
    validateMainMessage(message);
    if (webglRecoveryPromise !== null) await webglRecoveryPromise;
    switch (message.type) {
      case "init":
        await initialize(message);
        return;
      case "prepare":
        await prepare(message);
        return;
      case "start_engine":
        startEngine(message);
        return;
      case "attach_engine":
        attachEngine(message);
        return;
      case "switch_engine":
        switchEngine(message);
        return;
      case "rebuild_engine":
        rebuildEngine(message);
        return;
      case "resize":
        resize(message);
        return;
      case "metrics":
        if (!drainGpuDiagnostics()) return;
        respond(message.requestId, { type: "metrics", metrics: currentMetrics() });
        return;
      case "stop":
        stop();
        return;
      default:
        throw new Error(`unknown authoring render command ${message.type}`);
    }
  } catch (error) {
    fail(error, message?.requestId ?? null);
  }
}

async function initialize(message) {
  validateEnginePort(message.port, "authoring render init");
  await prepareSurface(message, "authoring render init");
  mode = validateMode(message.mode ?? MODE_LEGACY);
  attachRenderPort(message.port);
}

async function prepare(message) {
  await prepareSurface(message, "authoring render prepare");
  respond(message.requestId, {
    type: "prepared",
    transportMode,
    width,
    height,
  });
}

async function prepareSurface(message, operation) {
  if (renderPort !== null || canvas !== null) {
    throw new Error("authoring render worker is already initialized");
  }
  if (!(message.canvas instanceof OffscreenCanvas)) {
    throw new Error(`${operation} requires an OffscreenCanvas`);
  }
  validateTransportMode(message.transportMode);

  canvas = message.canvas;
  width = normalizedDimension(message.width ?? canvas.width);
  height = normalizedDimension(message.height ?? canvas.height);
  transportMode = message.transportMode;
  await init();
  canvas.addEventListener("webglcontextrestored", wakeAfterWebGlContextRestored);
}

function wakeAfterWebGlContextRestored() {
  // Rust records context restoration synchronously. Defer the platform wake
  // until every restore listener has run, then rebuild before presenting even
  // when the execution owner has settled to idle.
  queueMicrotask(() => void recoverAndPresentWebGlContext());
}

async function recoverAndPresentWebGlContext() {
  const restoringRenderer = renderer;
  if (stopped || restoringRenderer === null || webglRecoveryPromise !== null) return;
  const recovery = Promise.resolve(restoringRenderer.recoverWebGlContext?.());
  webglRecoveryPromise = recovery;
  try {
    await recovery;
    if (stopped || renderer !== restoringRenderer) return;
    webglRecoveryPromise = null;
    renderer.resize(width, height);
    if (!drainGpuDiagnostics()) return;
    needsPresent = true;
    if (tryPresent()) {
      flushBootstrapQueue();
      drainTransport();
    }
    if (needsPresent) scheduleFrame();
  } catch (error) {
    fail(error, null);
  } finally {
    if (webglRecoveryPromise === recovery) webglRecoveryPromise = null;
  }
}

function startEngine(message) {
  if (canvas === null || transportMode === null) {
    throw new Error("authoring render worker cannot start an engine before prepare");
  }
  if (
    renderPort !== null ||
    renderer !== null ||
    mode !== null ||
    bootstrapPromise !== null ||
    transitionRequestId !== null ||
    transitionMode !== null
  ) {
    throw new Error("authoring render worker can start its initial engine only once");
  }
  validateEnginePort(message.port, "authoring render initial engine");
  validateMatchingTransport(message.transportMode, "authoring render initial engine");

  mode = validateMode(message.mode);
  transitionRequestId = validateRequestId(message.requestId);
  transitionResponseType = "engine_started";
  attachRenderPort(message.port);
}

function attachEngine(message) {
  requireBootstrappedRenderer("reconnect");
  validateEnginePort(message.port, "authoring render reconnect");
  validateMatchingTransport(message.transportMode, "authoring render reconnect");
  const requestedMode = validateMode(message.mode ?? mode);
  if (requestedMode !== mode) {
    throw new Error(
      `authoring render reconnect mode ${requestedMode} does not match active mode ${mode}`,
    );
  }

  detachRenderPort();
  resetTransportState();
  if (mode === MODE_RETAINED) {
    reconnectResourceBundlePending = true;
  }
  attachRenderPort(message.port);
  scheduleFrame();
  respond(message.requestId, {
    type: "engine_port_attached",
    mode,
    ...modeFlags(),
    transportMode,
    backend: renderer.rendererBackend(),
    gpuGeneration: renderer.gpuGeneration(),
  });
}

function switchEngine(message) {
  requireBootstrappedRenderer("switch mode");
  const nextMode = validateMode(message.mode);
  if (nextMode === mode) {
    throw new Error(
      `authoring render mode switch requires a different mode; ${mode} is already active`,
    );
  }
  beginRendererTransition(message, nextMode, "mode_switched");
  modeSwitches += 1;
}

function rebuildEngine(message) {
  requireBootstrappedRenderer("rebuild renderer");
  const requestedMode = validateMode(message.mode ?? mode);
  if (requestedMode !== mode) {
    throw new Error(
      `authoring render rebuild mode ${requestedMode} does not match active mode ${mode}`,
    );
  }
  beginRendererTransition(message, mode, "renderer_rebuilt");
  rendererRebuilds += 1;
}

function beginRendererTransition(message, nextMode, responseType) {
  validateEnginePort(message.port, "authoring render engine transition");
  validateMatchingTransport(message.transportMode, "authoring render engine transition");
  if (
    transitionRequestId !== null ||
    transitionMode !== null ||
    bootstrapPromise !== null
  ) {
    throw new Error("authoring render engine transition is already in progress");
  }

  detachRenderPort();
  resetTransportState();
  transitionFrameLoopWasRunning = running;
  frameLoopGeneration += 1;
  running = false;
  reconnectResourceBundlePending = false;
  transitionMode = nextMode;
  transitionResourceBytes = null;
  transitionRequestId = validateRequestId(message.requestId);
  transitionResponseType = responseType;
  attachRenderPort(message.port);
}

function resize(message) {
  const nextWidth = normalizedDimension(message.width);
  const nextHeight = normalizedDimension(message.height);
  const dimensionsChanged = nextWidth !== width || nextHeight !== height;
  width = nextWidth;
  height = nextHeight;
  if (renderer === null) {
    return;
  }
  if (webglRecoveryPromise !== null) {
    needsPresent = true;
    return;
  }
  renderer.resize(width, height);
  if (!drainGpuDiagnostics()) return;
  // Surface changes need a presentation even when a semantic continuation has
  // completed and its engine is idle. Redundant resizes must remain a no-op:
  // render() correctly returns false while an unnecessary needsPresent would
  // wedge transport backpressure.
  if (dimensionsChanged) {
    needsPresent = true;
    tryPresent();
    if (engineWake !== null) scheduleFrame();
  }
}

function stop() {
  stopped = true;
  frameLoopGeneration += 1;
  running = false;
  bootstrapQueue = [];
  transitionMode = null;
  transitionResourceBytes = null;
  transitionFrameLoopWasRunning = false;
  detachRenderPort();
  disposeRenderer();
  self.close();
}

function attachRenderPort(port) {
  renderPort = port;
  port.addEventListener("message", (event) => {
    if (renderPort !== port) {
      return;
    }
    handleEngineMessage(event.data);
  });
  if (transportMode === EXECUTION_TRANSPORT_TRANSFERABLE) {
    transferableReceiver = new TransferableExecutionDeltaReceiver(
      port,
      (json, metadata) => (renderPort === port ? consumeDelta(json, metadata) : true),
    );
  }
  port.start();
}

function detachRenderPort() {
  cancelScheduledFrame();
  engineWake = null;
  renderPort?.close?.();
  renderPort = null;
}

function resetTransportState() {
  sharedReader = null;
  transferableReceiver = null;
  bootstrapQueue = [];
  bootstrapPromise = null;
  pendingPresentationPublication = null;
  lastPresentedPublication = null;
  pendingRendererObservationRequest = null;
  pendingRendererObservationPublication = null;
}

function handleEngineMessage(message) {
  if (!message || typeof message !== "object") {
    return;
  }
  if (message.type === "execution_wake") {
    try {
      const { cadence, timerAfterMilliseconds } = message;
      if (!["animation_frame", "timer", "idle"].includes(cadence) ||
          (cadence === "timer" &&
           (!Number.isFinite(timerAfterMilliseconds) || timerAfterMilliseconds < 0))) {
        throw new Error("invalid semantic execution wake directive");
      }
      engineWake = {
        cadence,
        deadline: cadence === "timer" ? performance.now() + timerAfterMilliseconds : null,
      };
      scheduleFrame();
    } catch (error) {
      fail(error, null);
    }
    return;
  }
  if (message.type === "retained_resources") {
    handleRetainedResources(message);
    return;
  }
  if (message.type === "transport_setup") {
    if (transportMode !== EXECUTION_TRANSPORT_SHARED || message.mode !== transportMode) {
      fail(new Error("authoring render worker received an unexpected shared transport setup"), null);
      return;
    }
    try {
      sharedReader = new SharedExecutionDeltaReader(message.mailbox);
      drainTransport();
    } catch (error) {
      fail(error, null);
    }
    return;
  }
  if (message.type === "shared_delta") {
    drainTransport();
    return;
  }
  if (message.type === "renderer_observation_request") {
    try {
      receiveRendererObservationRequest(message);
    } catch (error) {
      fail(error, null);
    }
    return;
  }
  if (message.type === "renderer_observation_cancel") {
    try {
      cancelRendererObservationRequest(message);
    } catch (error) {
      fail(error, null);
    }
  }
}

function handleRetainedResources(message) {
  try {
    if (!(message.bytes instanceof Uint8Array) || message.bytes.byteLength === 0) {
      throw new Error("retained resource bundle must be a non-empty Uint8Array");
    }
    if (transitionMode !== null) {
      if (transitionMode !== MODE_RETAINED) {
        throw new Error("legacy authoring render transition cannot accept retained resources");
      }
      if (transitionResourceBytes !== null || bootstrapPromise !== null) {
        throw new Error(
          "retained transition resource bundle may be installed only once before the snapshot",
        );
      }
      transitionResourceBytes = message.bytes;
      return;
    }
    if (mode !== MODE_RETAINED) {
      throw new Error("legacy authoring render mode cannot accept retained resources");
    }
    if (renderer !== null) {
      if (!reconnectResourceBundlePending) {
        throw new Error("retained resource bundle may be installed only once before the snapshot");
      }
      reconnectResourceBundlePending = false;
      return;
    }
    if (resourceBytes !== null || bootstrapPromise !== null) {
      throw new Error("retained resource bundle may be installed only once before the snapshot");
    }
    resourceBytes = message.bytes;
  } catch (error) {
    fail(error, null);
  }
}

function drainTransport() {
  try {
    if (sharedReader !== null) {
      const drained = sharedReader.drain((json, metadata) => consumeDelta(json, metadata));
      if (drained > 0) {
        renderPort?.postMessage({ type: "transport_writable" });
      }
    }
    transferableReceiver?.drain();
  } catch (error) {
    fail(error, null);
  }
}

function consumeDelta(json, publication = null) {
  if (transitionMode !== null) {
    return commitRendererTransition(json, publication);
  }
  if (renderer === null) {
    if (mode === MODE_RETAINED && resourceBytes === null) {
      throw new Error("retained authoring snapshot arrived before its resource bundle");
    }
    if (bootstrapPromise === null) {
      bootstrapPromise = bootstrapRenderer(json, true, publication);
      return true;
    }
    if (bootstrapQueue.length >= BOOTSTRAP_QUEUE_LIMIT) {
      return false;
    }
    bootstrapQueue.push({ json, publication });
    return true;
  }

  if (webglRecoveryPromise !== null) {
    return false;
  }

  if (mode === MODE_RETAINED && reconnectResourceBundlePending) {
    throw new Error("retained authoring reconnect snapshot arrived before its resource bundle");
  }
  if (needsPresent) {
    return false;
  }
  const applied = renderer.applyDeltaJson(json);
  if (!applied) {
    acknowledgeAlreadyPresented(publication);
    return true;
  }
  armRendererObservation(publication);
  pendingPresentationPublication = publication;
  needsPresent = true;
  tryPresent();
  if (engineWake !== null) scheduleFrame();
  return true;
}

function commitRendererTransition(initial, publication = null) {
  const nextMode = transitionMode;
  if (nextMode === null) {
    throw new Error("authoring render transition has no pending mode");
  }
  if (nextMode === MODE_RETAINED && transitionResourceBytes === null) {
    throw new Error("retained authoring transition snapshot arrived before its resource bundle");
  }

  const nextResourceBytes = transitionResourceBytes;
  const resumeFrameLoop = transitionFrameLoopWasRunning;
  transitionMode = null;
  transitionResourceBytes = null;
  transitionFrameLoopWasRunning = false;

  // The replacement engine has already queued a complete bootstrap payload.
  // Retire the currently presenting renderer only once that payload reaches
  // this worker; renderer/device construction is the only remaining blank-window seam.
  disposeRenderer();
  resourceBytes = nextResourceBytes;
  reconnectResourceBundlePending = false;
  needsPresent = false;
  pendingPresentationPublication = null;
  lastPresentedPublication = null;
  pendingRendererObservationPublication = null;
  mode = nextMode;
  bootstrapPromise = bootstrapRenderer(initial, resumeFrameLoop, publication);
  return true;
}

async function bootstrapRenderer(initial, resumeFrameLoop = true, publication = null) {
  const bootstrapGeneration = frameLoopGeneration;
  try {
    let createdRenderer;
    if (mode === MODE_RETAINED) {
      createdRenderer = await RetainedExecutionCanvasRenderer.create(canvas, resourceBytes);
      if (stopped) {
        createdRenderer.free?.();
        return;
      }
      renderer = createdRenderer;
      resourceBytes = null;
      const applied = renderer.applyDeltaJson(initial);
      if (!applied) {
        throw new Error("retained authoring renderer must begin from an applied snapshot");
      }
      armRendererObservation(publication);
    } else {
      createdRenderer = await ExecutionCanvasRenderer.create(canvas, initial);
      if (stopped) {
        createdRenderer.free?.();
        return;
      }
      renderer = createdRenderer;
    }
    renderer.resize(width, height);
    if (!drainGpuDiagnostics()) return;
    pendingPresentationPublication = publication;
    needsPresent = true;
    while (!tryPresent()) {
      if (
        stopped ||
        bootstrapGeneration !== frameLoopGeneration ||
        !drainGpuDiagnostics()
      ) {
        return;
      }
      await nextRenderOpportunity();
      if (stopped || bootstrapGeneration !== frameLoopGeneration) {
        return;
      }
    }
    if (stopped || bootstrapGeneration !== frameLoopGeneration || !drainGpuDiagnostics()) return;

    const ready = {
      mode,
      ...modeFlags(),
      transportMode,
      backend: renderer.rendererBackend(),
      gpuGeneration: renderer.gpuGeneration(),
      time: renderer.time(),
      presentedFrames,
    };
    if (mode === MODE_RETAINED) {
      ready.preloadedGeometryCount = renderer.preloadedGeometryCount();
      ready.preloadBytesUploaded = renderer.preloadBytesUploaded();
    }
    if (transitionRequestId === null) {
      postMain({ type: "ready", ...ready });
    } else {
      const requestId = transitionRequestId;
      const responseType = transitionResponseType;
      transitionRequestId = null;
      transitionResponseType = null;
      respond(requestId, { type: responseType, ...ready });
    }

    flushBootstrapQueue();
    drainTransport();
    if (resumeFrameLoop) {
      running = true;
      scheduleFrame(bootstrapGeneration);
    } else {
      running = false;
    }
  } catch (error) {
    const requestId = transitionRequestId;
    transitionRequestId = null;
    transitionResponseType = null;
    fail(error, requestId);
  } finally {
    bootstrapPromise = null;
  }
}

function tryPresent() {
  if (
    renderer === null ||
    webglRecoveryPromise !== null ||
    !needsPresent ||
    !drainGpuDiagnostics()
  ) {
    return false;
  }
  if (!renderer.render()) {
    drainGpuDiagnostics();
    return false;
  }
  needsPresent = false;
  presentedFrames += 1;
  const publication = pendingPresentationPublication;
  const observationPublication = pendingRendererObservationPublication;
  pendingPresentationPublication = null;
  pendingRendererObservationPublication = null;
  if (publication !== null) {
    lastPresentedPublication = publication;
  }
  try {
    acknowledgeRendererObservation(observationPublication, publication);
  } catch (error) {
    fail(error, null);
    return false;
  }
  acknowledgePresented(publication);
  return drainGpuDiagnostics();
}

function samePublication(left, right) {
  return left !== null && right !== null &&
    left.session === right.session && left.sequence === right.sequence;
}

function acknowledgePresented(publication) {
  if (publication === null || renderPort === null) {
    return;
  }
  renderPort.postMessage({
    type: "execution_presented",
    session: publication.session,
    sequence: publication.sequence,
  });
}

function acknowledgeRendererObservation(observationPublication, presentedPublication) {
  if (observationPublication === null) {
    return;
  }
  if (!samePublication(observationPublication, presentedPublication) || renderPort === null) {
    throw new Error("renderer observation presentation does not match its publication");
  }
  const json = renderer.takeRendererObservationJson();
  if (typeof json !== "string") {
    throw new Error("retained renderer did not publish its requested observation");
  }
  renderPort.postMessage({
    type: "renderer_observation",
    session: observationPublication.session,
    sequence: observationPublication.sequence,
    json,
  });
}

function acknowledgeAlreadyPresented(publication) {
  // `applyDeltaJson` returns false only for a typed stale transport envelope.
  // It cannot prove a new publication reached the surface. A duplicate of the
  // exact already-presented envelope is safe to acknowledge without redrawing;
  // any older or foreign envelope remains unacknowledged.
  if (!needsPresent && samePublication(publication, lastPresentedPublication)) {
    acknowledgePresented(publication);
  }
}

function flushBootstrapQueue() {
  if (renderer === null) {
    return;
  }
  while (!needsPresent && bootstrapQueue.length > 0) {
    const { json, publication } = bootstrapQueue.shift();
    const applied = renderer.applyDeltaJson(json);
    if (!applied) {
      acknowledgeAlreadyPresented(publication);
      continue;
    }
    armRendererObservation(publication);
    pendingPresentationPublication = publication;
    needsPresent = true;
    if (!tryPresent()) {
      break;
    }
  }
}

function receiveRendererObservationRequest(message) {
  const publication = rendererObservationMessagePublication(message);
  if (mode !== MODE_RETAINED) {
    throw new Error("renderer observations require retained execution");
  }
  if (pendingRendererObservationRequest !== null ||
      pendingRendererObservationPublication !== null) {
    throw new Error("render worker already has a pending renderer observation");
  }
  if (typeof message.json !== "string") {
    throw new Error("renderer observation request must be JSON");
  }
  pendingRendererObservationRequest = { publication, json: message.json };
}

function cancelRendererObservationRequest(message) {
  const publication = rendererObservationMessagePublication(message);
  if (pendingRendererObservationRequest !== null &&
      samePublication(pendingRendererObservationRequest.publication, publication)) {
    pendingRendererObservationRequest = null;
  }
}

function rendererObservationMessagePublication(message) {
  if (!Number.isSafeInteger(message?.session) || message.session < 0 ||
      !Number.isSafeInteger(message?.sequence) || message.sequence < 0) {
    throw new Error("renderer observation publication is invalid");
  }
  return { session: message.session, sequence: message.sequence };
}

function armRendererObservation(publication) {
  if (pendingRendererObservationRequest === null) {
    return;
  }
  const requested = pendingRendererObservationRequest.publication;
  if (!samePublication(requested, publication)) {
    if (publication !== null &&
        (publication.session !== requested.session || publication.sequence > requested.sequence)) {
      throw new Error("renderer observation publication was skipped or replaced");
    }
    return;
  }
  if (renderer === null || typeof renderer.setRendererObservationRequestJson !== "function") {
    throw new Error("retained renderer observation support is unavailable");
  }
  renderer.setRendererObservationRequestJson(pendingRendererObservationRequest.json);
  pendingRendererObservationPublication = publication;
  pendingRendererObservationRequest = null;
}

function nextRenderOpportunity() {
  return new Promise((resolve) => {
    if (typeof self.requestAnimationFrame === "function") {
      self.requestAnimationFrame(resolve);
    } else {
      setTimeout(() => resolve(performance.now()), 16);
    }
  });
}

function cancelScheduledFrame() {
  scheduleTicket += 1;
  if (scheduledFrame !== null) {
    if (scheduledFrame.kind === "animation") {
      self.cancelAnimationFrame?.(scheduledFrame.handle);
    } else {
      clearTimeout(scheduledFrame.handle);
    }
    scheduledFrame = null;
  }
}

function scheduleFrame(generation = frameLoopGeneration) {
  cancelScheduledFrame();
  if (!running || webglRecoveryPromise !== null) return;
  const needsAnimationFrame = needsPresent || engineWake === null ||
    engineWake.cadence === "animation_frame";
  if (!needsAnimationFrame && engineWake.cadence === "idle") return;
  const ticket = scheduleTicket;
  if (needsAnimationFrame && typeof self.requestAnimationFrame === "function") {
    scheduledFrame = {
      kind: "animation",
      handle: self.requestAnimationFrame((timestamp) => frame(timestamp, generation, ticket)),
    };
  } else {
    const delay = needsAnimationFrame ? 16 : Math.max(0, engineWake.deadline - performance.now());
    scheduledFrame = {
      kind: "timer",
      handle: setTimeout(() => frame(performance.now(), generation, ticket), delay),
    };
  }
}

function frame(timestamp, generation, ticket) {
  if (ticket !== scheduleTicket || generation !== frameLoopGeneration || !running) return;
  scheduledFrame = null;
  if (webglRecoveryPromise !== null || !drainGpuDiagnostics()) return;
  lastFrameTimestamp = timestamp;
  if (needsPresent && tryPresent()) {
    flushBootstrapQueue();
  }
  drainTransport();
  if (!needsPresent) {
    flushBootstrapQueue();
    drainTransport();
  }
  if (!running || !drainGpuDiagnostics()) return;
  const tickDue = engineWake === null || engineWake.cadence === "animation_frame" ||
    (engineWake.cadence === "timer" && performance.now() >= engineWake.deadline);
  if (tickDue) {
    // One directive admits one engine drive. The response supplies the next
    // Rust-derived directive, so an idle/waiting continuation never RAF-polls.
    if (engineWake !== null) engineWake = { cadence: "idle", deadline: null };
    renderPort?.postMessage({ type: "tick", timestamp });
  }
  scheduleFrame(generation);
}

function drainGpuDiagnostics() {
  if (renderer === null) return true;
  try {
    return drainRendererGpuDiagnostics(renderer, {
      onRecoverable(diagnostic) {
        postMain({
          type: "recoverable_error",
          owner: "render",
          message: formatGpuDiagnostic(diagnostic),
          diagnostic,
        });
      },
      onFatal(diagnostic) {
        fail(new Error(formatGpuDiagnostic(diagnostic)), null);
      },
    });
  } catch (error) {
    fail(error, null);
    return false;
  }
}

function currentMetrics() {
  const base = {
    ready: renderer !== null,
    mode,
    ...modeFlags(),
    transportMode,
    presentedFrames,
    modeSwitches,
    rendererRebuilds,
    transitionMode,
    lastFrameTimestamp,
    bufferedDeltas: bootstrapQueue.length + (transferableReceiver?.pendingCount() ?? 0),
    needsPresent,
  };
  if (renderer === null) {
    return {
      ...base,
      resourceBundlePending: mode === MODE_RETAINED && resourceBytes !== null,
    };
  }
  const metrics = {
    ...base,
    backend: renderer.rendererBackend(),
    gpuGeneration: renderer.gpuGeneration(),
    time: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    instancesDrawn: renderer.lastInstancesDrawn(),
    bytesUploaded: renderer.lastBytesUploaded(),
    geometryCacheMisses: renderer.lastGeometryCacheMisses(),
  };
  if (mode === MODE_RETAINED) {
    metrics.outlineCacheMisses = renderer.lastOutlineCacheMisses();
    metrics.resourceBundlePending = reconnectResourceBundlePending;
    metrics.preloadedGeometryCount = renderer.preloadedGeometryCount();
    metrics.preloadBytesUploaded = renderer.preloadBytesUploaded();
  }
  return metrics;
}

function disposeRenderer() {
  if (renderer === null) {
    return;
  }
  renderer.free?.();
  renderer = null;
}

function modeFlags() {
  return mode === MODE_RETAINED ? { retained: true, mixed: true } : {};
}

function requireBootstrappedRenderer(operation) {
  if (renderer === null || renderPort === null) {
    throw new Error(`authoring render worker cannot ${operation} before renderer bootstrap`);
  }
}

function validateEnginePort(port, operation) {
  if (!(port instanceof MessagePort)) {
    throw new Error(`${operation} requires an engine MessagePort`);
  }
}

function validateMatchingTransport(candidate, operation) {
  validateTransportMode(candidate);
  if (candidate !== transportMode) {
    throw new Error(`${operation} transport ${candidate} does not match ${transportMode}`);
  }
}

function validateTransportMode(candidate) {
  if (
    candidate !== EXECUTION_TRANSPORT_SHARED &&
    candidate !== EXECUTION_TRANSPORT_TRANSFERABLE
  ) {
    throw new Error(`unsupported authoring render transport mode ${candidate}`);
  }
}

function validateMode(candidate) {
  if (candidate !== MODE_LEGACY && candidate !== MODE_RETAINED) {
    throw new Error(`unsupported authoring render mode ${candidate}`);
  }
  return candidate;
}

function respond(requestId, payload) {
  postMain({ requestId: validateRequestId(requestId), ...payload });
}

function validateRequestId(requestId) {
  if (!Number.isSafeInteger(requestId) || requestId < 0) {
    throw new Error("render request ID must be a non-negative safe integer");
  }
  return requestId;
}

function fail(error, requestId) {
  frameLoopGeneration += 1;
  running = false;
  const effectiveRequestId = requestId ?? transitionRequestId;
  transitionRequestId = null;
  transitionResponseType = null;
  transitionMode = null;
  transitionResourceBytes = null;
  transitionFrameLoopWasRunning = false;
  const message = String(error?.message ?? error);
  renderPort?.postMessage({ type: "render_error", message });
  postMain({ type: "error", requestId: effectiveRequestId, message });
}

function postMain(payload) {
  self.postMessage({
    channel: RENDER_CHANNEL,
    protocolVersion: RENDER_PROTOCOL_VERSION,
    ...payload,
  });
}

function validateMainMessage(message) {
  if (
    !message ||
    typeof message !== "object" ||
    message.channel !== RENDER_CHANNEL ||
    message.protocolVersion !== RENDER_PROTOCOL_VERSION
  ) {
    throw new Error("invalid authoring render control envelope");
  }
}

function normalizedDimension(value) {
  if (!Number.isFinite(value)) {
    throw new Error(`invalid render surface dimension ${value}`);
  }
  return Math.max(1, Math.round(value));
}
