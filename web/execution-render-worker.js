import init, { ExecutionCanvasRenderer } from "./pkg/noon_web.js";
import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  SharedExecutionDeltaReader,
  TransferableExecutionDeltaReceiver,
} from "./execution-transport.js";

const RENDER_CHANNEL = "noon.render";
const RENDER_PROTOCOL_VERSION = 1;

let renderPort = null;
let transportMode = null;
let sharedReader = null;
let renderer = null;
let canvas = null;
let width = 1;
let height = 1;
let deltaQueue = [];
let bootstrapPromise = null;
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
      case "resize":
        width = normalizedDimension(message.width);
        height = normalizedDimension(message.height);
        renderer?.resize(width, height);
        return;
      case "metrics":
        respond(message.requestId, {
          type: "metrics",
          metrics: currentMetrics(),
        });
        return;
      case "stop":
        running = false;
        renderPort?.close?.();
        self.close();
        return;
      default:
        throw new Error(`unknown render command ${message.type}`);
    }
  } catch (error) {
    fail(error, message?.requestId ?? null);
  }
}

async function initialize(message) {
  if (renderPort !== null) {
    throw new Error("execution render worker is already initialized");
  }
  if (!(message.port instanceof MessagePort)) {
    throw new Error("execution render init requires an engine MessagePort");
  }
  if (!(message.canvas instanceof OffscreenCanvas)) {
    throw new Error("execution render init requires an OffscreenCanvas");
  }
  if (
    message.transportMode !== EXECUTION_TRANSPORT_SHARED &&
    message.transportMode !== EXECUTION_TRANSPORT_TRANSFERABLE
  ) {
    throw new Error(`unsupported execution transport mode ${message.transportMode}`);
  }

  canvas = message.canvas;
  width = normalizedDimension(message.width ?? canvas.width);
  height = normalizedDimension(message.height ?? canvas.height);
  transportMode = message.transportMode;
  renderPort = message.port;
  await init();

  renderPort.addEventListener("message", (event) => handleEngineMessage(event.data));
  if (transportMode === EXECUTION_TRANSPORT_TRANSFERABLE) {
    new TransferableExecutionDeltaReceiver(renderPort, (json) => enqueueDelta(json));
  }
  renderPort.start();
}

function handleEngineMessage(message) {
  if (!message || typeof message !== "object") {
    return;
  }
  if (message.type === "transport_setup") {
    if (transportMode !== EXECUTION_TRANSPORT_SHARED || message.mode !== transportMode) {
      fail(new Error("render worker received an unexpected shared transport setup"), null);
      return;
    }
    try {
      sharedReader = new SharedExecutionDeltaReader(message.mailbox);
      pumpSharedMailbox();
    } catch (error) {
      fail(error, null);
    }
    return;
  }
  if (message.type === "shared_delta") {
    pumpSharedMailbox();
  }
}

function pumpSharedMailbox() {
  if (sharedReader === null) {
    return;
  }
  try {
    const drained = sharedReader.drain((json) => enqueueDelta(json));
    if (drained > 0) {
      renderPort.postMessage({ type: "transport_writable" });
    }
  } catch (error) {
    fail(error, null);
  }
}

function enqueueDelta(json) {
  deltaQueue.push(json);
  if (bootstrapPromise === null && renderer === null) {
    bootstrapPromise = bootstrapRenderer();
    return;
  }
  if (renderer !== null) {
    flushDeltaQueue();
  }
}

async function bootstrapRenderer() {
  try {
    const initial = deltaQueue.shift();
    if (initial === undefined) {
      throw new Error("render worker bootstrap requires an execution snapshot");
    }
    renderer = await ExecutionCanvasRenderer.create(canvas, initial);
    renderer.resize(width, height);
    renderer.render();
    presentedFrames += 1;
    flushDeltaQueue();
    running = true;
    postMain({
      type: "ready",
      transportMode,
      backend: renderer.rendererBackend(),
    });
    scheduleFrame();
  } catch (error) {
    fail(error, null);
  }
}

function flushDeltaQueue() {
  if (renderer === null) {
    return;
  }
  while (deltaQueue.length > 0) {
    const json = deltaQueue.shift();
    const applied = renderer.applyDeltaJson(json);
    if (!applied) {
      continue;
    }
    if (renderer.render()) {
      presentedFrames += 1;
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
  if (!running) {
    return;
  }
  lastFrameTimestamp = timestamp;
  if (transportMode === EXECUTION_TRANSPORT_SHARED) {
    pumpSharedMailbox();
  }
  renderPort.postMessage({ type: "tick", timestamp });
  scheduleFrame();
}

function currentMetrics() {
  if (renderer === null) {
    return {
      ready: false,
      presentedFrames,
      lastFrameTimestamp,
    };
  }
  return {
    ready: true,
    transportMode,
    backend: renderer.rendererBackend(),
    time: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    instancesDrawn: renderer.lastInstancesDrawn(),
    bytesUploaded: renderer.lastBytesUploaded(),
    geometryCacheMisses: renderer.lastGeometryCacheMisses(),
    presentedFrames,
    lastFrameTimestamp,
  };
}

function respond(requestId, payload) {
  if (!Number.isSafeInteger(requestId) || requestId < 0) {
    throw new Error("render request ID must be a non-negative safe integer");
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
    throw new Error("invalid execution render control envelope");
  }
}

function normalizedDimension(value) {
  if (!Number.isFinite(value)) {
    throw new Error(`invalid render surface dimension ${value}`);
  }
  return Math.max(1, Math.round(value));
}
