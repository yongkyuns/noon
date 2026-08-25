import init, { EngineScenePlayer } from "./pkg/noon_web.js";
import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  SharedExecutionDeltaWriter,
  TransferableExecutionDeltaSender,
  createSharedExecutionMailbox,
} from "./execution-transport.js";

const ENGINE_CHANNEL = "noon.engine";
const ENGINE_PROTOCOL_VERSION = 1;

let renderPort = null;
let transportMode = null;
let transport = null;
let player = null;
let latestTick = null;
let controlQueue = [];
let initialized = false;
let stopped = false;

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
      case "replace_scene":
      case "reconcile_scene":
      case "apply_patch":
        enqueueControl(message);
        return;
      case "stop":
        stopped = true;
        controlQueue = [];
        latestTick = null;
        renderPort?.close?.();
        self.close();
        return;
      default:
        throw new Error(`unknown engine command ${message.type}`);
    }
  } catch (error) {
    fail(error, message?.requestId ?? null);
  }
}

async function initialize(message) {
  if (initialized) {
    throw new Error("execution engine worker is already initialized");
  }
  if (!(message.port instanceof MessagePort)) {
    throw new Error("execution engine init requires a render MessagePort");
  }
  if (typeof message.sceneJson !== "string") {
    throw new Error("execution engine init requires scene JSON");
  }
  if (!Number.isFinite(message.loopDurationSeconds) || message.loopDurationSeconds <= 0) {
    throw new Error("execution engine loop duration must be positive and finite");
  }
  if (!Number.isSafeInteger(message.session) || message.session < 0) {
    throw new Error("execution engine session must be a non-negative safe integer");
  }
  if (
    message.transportMode !== EXECUTION_TRANSPORT_SHARED &&
    message.transportMode !== EXECUTION_TRANSPORT_TRANSFERABLE
  ) {
    throw new Error(`unsupported execution transport mode ${message.transportMode}`);
  }

  renderPort = message.port;
  renderPort.addEventListener("message", (event) => handleRenderMessage(event.data));
  renderPort.start();
  transportMode = message.transportMode;
  await init();
  player = new EngineScenePlayer(
    message.sceneJson,
    message.loopDurationSeconds,
    message.session,
  );

  if (transportMode === EXECUTION_TRANSPORT_SHARED) {
    const mailbox = createSharedExecutionMailbox(message.sharedSlotCapacity ?? 1024 * 1024);
    transport = new SharedExecutionDeltaWriter(mailbox);
    renderPort.postMessage({
      type: "transport_setup",
      mode: EXECUTION_TRANSPORT_SHARED,
      mailbox,
    });
  } else {
    transport = new TransferableExecutionDeltaSender(renderPort, {
      maxInFlight: 2,
      onWritable: drainWork,
    });
  }

  const initial = player.initialDeltaJson();
  sendDeltaOrThrow(initial);
  initialized = true;
  postMain({ type: "ready", transportMode });
  drainWork();
}

function handleRenderMessage(message) {
  if (!message || typeof message !== "object") {
    return;
  }
  if (message.type === "tick") {
    if (!Number.isFinite(message.timestamp)) {
      fail(new Error("render worker sent a non-finite frame timestamp"), null);
      return;
    }
    latestTick = message.timestamp;
    drainWork();
    return;
  }
  if (message.type === "transport_writable") {
    drainWork();
    return;
  }
  if (message.type === "render_error") {
    fail(new Error(`render worker failed: ${message.message}`), null);
  }
}

function enqueueControl(message) {
  requireInitialized();
  if (!Number.isSafeInteger(message.requestId) || message.requestId < 0) {
    throw new Error("engine request ID must be a non-negative safe integer");
  }
  controlQueue.push(message);
  drainWork();
}

function drainWork() {
  if (!initialized || stopped || transport === null || player === null) {
    return;
  }

  while (controlQueue.length > 0 && transportCanSend()) {
    const message = controlQueue.shift();
    try {
      executeControl(message);
    } catch (error) {
      fail(error, message.requestId);
      return;
    }
  }

  if (controlQueue.length > 0 || latestTick === null || !transportCanSend()) {
    return;
  }

  const timestamp = latestTick;
  latestTick = null;
  try {
    const delta = player.tickDeltaJson(timestamp);
    if (delta !== undefined && delta !== null) {
      sendDeltaOrThrow(delta);
    }
  } catch (error) {
    fail(error, null);
  }
}

function executeControl(message) {
  switch (message.type) {
    case "replace_scene": {
      const delta = player.replaceSceneDeltaJson(message.sceneJson);
      sendDeltaOrThrow(delta);
      respond(message.requestId, {
        type: "result",
        operation: "replace_scene",
        time: player.time(),
        nextPatchSequence: String(player.nextPatchSequence()),
        sceneJson: player.sceneJson(),
      });
      return;
    }
    case "reconcile_scene": {
      const result = JSON.parse(player.reconcileSceneDeltaJson(message.sceneJson));
      if (result.delta !== null && result.delta !== undefined) {
        sendDeltaOrThrow(result.delta);
      }
      respond(message.requestId, {
        type: "result",
        operation: "reconcile_scene",
        incremental: result.incremental,
        time: player.time(),
        nextPatchSequence: String(player.nextPatchSequence()),
        sceneJson: player.sceneJson(),
      });
      return;
    }
    case "apply_patch": {
      const delta = player.applyPatchBatchDeltaJson(message.patchBatchJson);
      if (delta !== undefined && delta !== null) {
        sendDeltaOrThrow(delta);
      }
      respond(message.requestId, {
        type: "result",
        operation: "apply_patch",
        time: player.time(),
        nextPatchSequence: String(player.nextPatchSequence()),
        sceneJson: player.sceneJson(),
      });
      return;
    }
    default:
      throw new Error(`unknown queued engine command ${message.type}`);
  }
}

function transportCanSend() {
  if (transport === null) {
    return false;
  }
  if (typeof transport.canSend === "function") {
    return transport.canSend();
  }
  if (typeof transport.inFlight === "function") {
    return transport.inFlight() < 2;
  }
  return true;
}

function sendDeltaOrThrow(json) {
  if (json === undefined || json === null) {
    return;
  }
  if (!transport.send(json)) {
    throw new Error("execution transport became backpressured after work was admitted");
  }
  if (transportMode === EXECUTION_TRANSPORT_SHARED) {
    renderPort.postMessage({ type: "shared_delta" });
  }
}

function respond(requestId, payload) {
  if (!Number.isSafeInteger(requestId) || requestId < 0) {
    throw new Error("engine request ID must be a non-negative safe integer");
  }
  postMain({ requestId, ...payload });
}

function fail(error, requestId) {
  const message = String(error?.message ?? error);
  postMain({ type: "error", requestId, message });
}

function postMain(payload) {
  self.postMessage({
    channel: ENGINE_CHANNEL,
    protocolVersion: ENGINE_PROTOCOL_VERSION,
    ...payload,
  });
}

function validateMainMessage(message) {
  if (
    !message ||
    typeof message !== "object" ||
    message.channel !== ENGINE_CHANNEL ||
    message.protocolVersion !== ENGINE_PROTOCOL_VERSION
  ) {
    throw new Error("invalid execution engine control envelope");
  }
}

function requireInitialized() {
  if (!initialized || player === null || transport === null) {
    throw new Error("execution engine worker is not initialized");
  }
}
