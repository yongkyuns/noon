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
let needsPresent = false;
let running = false;
let lastFrameTimestamp = null;
let presentedFrames = 0;
let modeSwitches = 0;
let rendererRebuilds = 0;

self.addEventListener("message", (event) => {
  void handleMainMessage(event.data);
});

async function handleMainMessage(message) {
  try {
    validateMainMessage(message);
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
  reconnectResourceBundlePending = false;
  transitionMode = nextMode;
  transitionResourceBytes = null;
  transitionRequestId = validateRequestId(message.requestId);
  transitionResponseType = responseType;
  attachRenderPort(message.port);
}

function resize(message) {
  width = normalizedDimension(message.width);
  height = normalizedDimension(message.height);
  if (renderer === null) {
    return;
  }
  renderer.resize(width, height);
  if (!drainGpuDiagnostics()) return;
  if (mode === MODE_RETAINED) {
    needsPresent = true;
    tryPresent();
  }
}

function stop() {
  running = false;
  bootstrapQueue = [];
  transitionMode = null;
  transitionResourceBytes = null;
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
      (json) => (renderPort === port ? consumeDelta(json) : true),
    );
  }
  port.start();
}

function detachRenderPort() {
  renderPort?.close?.();
  renderPort = null;
}

function resetTransportState() {
  sharedReader = null;
  transferableReceiver = null;
  bootstrapQueue = [];
  bootstrapPromise = null;
}

function handleEngineMessage(message) {
  if (!message || typeof message !== "object") {
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
      const drained = sharedReader.drain((json) => consumeDelta(json));
      if (drained > 0) {
        renderPort?.postMessage({ type: "transport_writable" });
      }
    }
    transferableReceiver?.drain();
  } catch (error) {
    fail(error, null);
  }
}

function consumeDelta(json) {
  if (transitionMode !== null) {
    return commitRendererTransition(json);
  }
  if (renderer === null) {
    if (mode === MODE_RETAINED && resourceBytes === null) {
      throw new Error("retained authoring snapshot arrived before its resource bundle");
    }
    if (bootstrapPromise === null) {
      bootstrapPromise = bootstrapRenderer(json);
      return true;
    }
    if (bootstrapQueue.length >= BOOTSTRAP_QUEUE_LIMIT) {
      return false;
    }
    bootstrapQueue.push(json);
    return true;
  }

  if (mode === MODE_RETAINED && reconnectResourceBundlePending) {
    throw new Error("retained authoring reconnect snapshot arrived before its resource bundle");
  }
  if (needsPresent) {
    return false;
  }
  const applied = renderer.applyDeltaJson(json);
  if (!applied) {
    return true;
  }
  needsPresent = true;
  tryPresent();
  return true;
}

function commitRendererTransition(initial) {
  const nextMode = transitionMode;
  if (nextMode === null) {
    throw new Error("authoring render transition has no pending mode");
  }
  if (nextMode === MODE_RETAINED && transitionResourceBytes === null) {
    throw new Error("retained authoring transition snapshot arrived before its resource bundle");
  }

  const nextResourceBytes = transitionResourceBytes;
  transitionMode = null;
  transitionResourceBytes = null;

  // The replacement engine has already queued a complete bootstrap payload.
  // Retire the currently presenting renderer only once that payload reaches
  // this worker; renderer/device construction is the only remaining blank-window seam.
  disposeRenderer();
  resourceBytes = nextResourceBytes;
  reconnectResourceBundlePending = false;
  needsPresent = false;
  mode = nextMode;
  bootstrapPromise = bootstrapRenderer(initial);
  return true;
}

async function bootstrapRenderer(initial) {
  const wasRunning = running;
  try {
    if (mode === MODE_RETAINED) {
      renderer = await RetainedExecutionCanvasRenderer.create(canvas, resourceBytes);
      resourceBytes = null;
      const applied = renderer.applyDeltaJson(initial);
      if (!applied) {
        throw new Error("retained authoring renderer must begin from an applied snapshot");
      }
    } else {
      renderer = await ExecutionCanvasRenderer.create(canvas, initial);
    }
    renderer.resize(width, height);
    if (!drainGpuDiagnostics()) return;
    needsPresent = true;
    running = true;
    tryPresent();
    if (!running || !drainGpuDiagnostics()) return;

    const ready = {
      mode,
      ...modeFlags(),
      transportMode,
      backend: renderer.rendererBackend(),
      gpuGeneration: renderer.gpuGeneration(),
    };
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
    if (!wasRunning) {
      scheduleFrame();
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
  if (renderer === null || !needsPresent || !drainGpuDiagnostics()) {
    return false;
  }
  if (!renderer.render()) {
    drainGpuDiagnostics();
    return false;
  }
  needsPresent = false;
  presentedFrames += 1;
  return drainGpuDiagnostics();
}

function flushBootstrapQueue() {
  if (renderer === null) {
    return;
  }
  while (!needsPresent && bootstrapQueue.length > 0) {
    const json = bootstrapQueue.shift();
    const applied = renderer.applyDeltaJson(json);
    if (!applied) {
      continue;
    }
    needsPresent = true;
    if (!tryPresent()) {
      break;
    }
  }
}

function scheduleFrame() {
  if (!running) {
    return;
  }
  if (typeof self.requestAnimationFrame === "function") {
    self.requestAnimationFrame(frame);
  } else {
    setTimeout(() => frame(performance.now()), 16);
  }
}

function frame(timestamp) {
  if (!running || !drainGpuDiagnostics()) {
    return;
  }
  lastFrameTimestamp = timestamp;
  if (needsPresent && tryPresent()) {
    flushBootstrapQueue();
  }
  drainTransport();
  if (!needsPresent) {
    flushBootstrapQueue();
    drainTransport();
  }
  if (!running || !drainGpuDiagnostics()) {
    return;
  }
  renderPort?.postMessage({ type: "tick", timestamp });
  scheduleFrame();
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
  running = false;
  const effectiveRequestId = requestId ?? transitionRequestId;
  transitionRequestId = null;
  transitionResponseType = null;
  transitionMode = null;
  transitionResourceBytes = null;
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
