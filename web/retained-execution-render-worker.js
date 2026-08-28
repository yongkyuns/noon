import init, { RetainedExecutionCanvasRenderer } from "./pkg/noon_web.js";
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
const BOOTSTRAP_QUEUE_LIMIT = 1;

let renderPort = null;
let transportMode = null;
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
let needsPresent = false;
let running = false;
let lastFrameTimestamp = null;
let presentedFrames = 0;

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
      case "attach_engine":
        attachEngine(message);
        return;
      case "resize":
        width = normalizedDimension(message.width);
        height = normalizedDimension(message.height);
        if (renderer !== null) {
          renderer.resize(width, height);
          if (!drainGpuDiagnostics()) return;
          needsPresent = true;
          tryPresent();
        }
        return;
      case "metrics":
        if (!drainGpuDiagnostics()) return;
        respond(message.requestId, { type: "metrics", metrics: currentMetrics() });
        return;
      case "stop":
        running = false;
        bootstrapQueue = [];
        renderPort?.close?.();
        self.close();
        return;
      default:
        throw new Error(`unknown mixed retained render command ${message.type}`);
    }
  } catch (error) {
    fail(error, message?.requestId ?? null);
  }
}

async function initialize(message) {
  if (renderPort !== null) {
    throw new Error("mixed retained execution render worker is already initialized");
  }
  if (!(message.port instanceof MessagePort)) {
    throw new Error("mixed retained execution render init requires an engine MessagePort");
  }
  if (!(message.canvas instanceof OffscreenCanvas)) {
    throw new Error("mixed retained execution render init requires an OffscreenCanvas");
  }
  if (
    message.transportMode !== EXECUTION_TRANSPORT_SHARED &&
    message.transportMode !== EXECUTION_TRANSPORT_TRANSFERABLE
  ) {
    throw new Error(`unsupported mixed retained execution transport mode ${message.transportMode}`);
  }

  canvas = message.canvas;
  width = normalizedDimension(message.width ?? canvas.width);
  height = normalizedDimension(message.height ?? canvas.height);
  transportMode = message.transportMode;
  await init();
  attachRenderPort(message.port);
}

function attachEngine(message) {
  if (renderer === null || renderPort === null) {
    throw new Error("mixed retained render worker cannot reconnect before renderer bootstrap");
  }
  if (!(message.port instanceof MessagePort)) {
    throw new Error("mixed retained render reconnect requires an engine MessagePort");
  }
  if (message.transportMode !== transportMode) {
    throw new Error(
      `mixed retained render reconnect transport ${message.transportMode} does not match ${transportMode}`,
    );
  }

  renderPort.close?.();
  sharedReader = null;
  transferableReceiver = null;
  bootstrapQueue = [];
  reconnectResourceBundlePending = true;
  attachRenderPort(message.port);
  respond(message.requestId, {
    type: "engine_port_attached",
    transportMode,
    retained: true,
    mixed: true,
    backend: renderer.rendererBackend(),
    gpuGeneration: renderer.gpuGeneration(),
  });
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

function handleEngineMessage(message) {
  if (!message || typeof message !== "object") {
    return;
  }
  if (message.type === "retained_resources") {
    try {
      if (!(message.bytes instanceof Uint8Array) || message.bytes.byteLength === 0) {
        throw new Error("retained resource bundle must be a non-empty Uint8Array");
      }
      if (renderer !== null) {
        if (!reconnectResourceBundlePending) {
          throw new Error("retained resource bundle may be installed only once before the snapshot");
        }
        // A replacement engine deterministically recreates the same immutable
        // resource bundle. The live renderer already owns those GPU resources,
        // so acknowledge the reconnect generation without rebuilding them.
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
    return;
  }
  if (message.type === "transport_setup") {
    if (transportMode !== EXECUTION_TRANSPORT_SHARED || message.mode !== transportMode) {
      fail(new Error("mixed retained render worker received an unexpected shared transport setup"), null);
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

function drainTransport() {
  try {
    if (sharedReader !== null) {
      const drained = sharedReader.drain((json) => consumeDelta(json));
      if (drained > 0) {
        renderPort.postMessage({ type: "transport_writable" });
      }
    }
    transferableReceiver?.drain();
  } catch (error) {
    fail(error, null);
  }
}

function consumeDelta(json) {
  if (renderer === null) {
    if (resourceBytes === null) {
      throw new Error("mixed retained execution snapshot arrived before its resource bundle");
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

  if (reconnectResourceBundlePending) {
    throw new Error("mixed retained reconnect snapshot arrived before its resource bundle");
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

async function bootstrapRenderer(initial) {
  try {
    renderer = await RetainedExecutionCanvasRenderer.create(canvas, resourceBytes);
    resourceBytes = null;
    const applied = renderer.applyDeltaJson(initial);
    if (!applied) {
      throw new Error("mixed retained execution renderer must begin from an applied snapshot");
    }
    renderer.resize(width, height);
    if (!drainGpuDiagnostics()) return;
    needsPresent = true;
    running = true;
    tryPresent();
    if (!running || !drainGpuDiagnostics()) return;
    postMain({
      type: "ready",
      transportMode,
      retained: true,
      mixed: true,
      backend: renderer.rendererBackend(),
      gpuGeneration: renderer.gpuGeneration(),
    });
    flushBootstrapQueue();
    drainTransport();
    scheduleFrame();
  } catch (error) {
    fail(error, null);
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
  renderPort.postMessage({ type: "tick", timestamp });
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
  if (renderer === null) {
    return {
      ready: false,
      retained: true,
      mixed: true,
      presentedFrames,
      lastFrameTimestamp,
      bufferedDeltas: bootstrapQueue.length + (transferableReceiver?.pendingCount() ?? 0),
      needsPresent,
      resourceBundlePending: resourceBytes !== null,
    };
  }
  return {
    ready: true,
    retained: true,
    mixed: true,
    transportMode,
    backend: renderer.rendererBackend(),
    gpuGeneration: renderer.gpuGeneration(),
    time: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    instancesDrawn: renderer.lastInstancesDrawn(),
    bytesUploaded: renderer.lastBytesUploaded(),
    geometryCacheMisses: renderer.lastGeometryCacheMisses(),
    outlineCacheMisses: renderer.lastOutlineCacheMisses(),
    presentedFrames,
    lastFrameTimestamp,
    bufferedDeltas: bootstrapQueue.length + (transferableReceiver?.pendingCount() ?? 0),
    needsPresent,
    resourceBundlePending: reconnectResourceBundlePending,
  };
}

function respond(requestId, payload) {
  if (!Number.isSafeInteger(requestId) || requestId < 0) {
    throw new Error("mixed retained render request ID must be a non-negative safe integer");
  }
  postMain({ requestId, ...payload });
}

function fail(error, requestId) {
  running = false;
  const message = String(error?.message ?? error);
  renderPort?.postMessage({ type: "render_error", message });
  postMain({ type: "error", requestId, message });
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
    throw new Error("invalid mixed retained execution render control envelope");
  }
}

function normalizedDimension(value) {
  if (!Number.isFinite(value)) {
    throw new Error(`invalid mixed retained render surface dimension ${value}`);
  }
  return Math.max(1, Math.round(value));
}
