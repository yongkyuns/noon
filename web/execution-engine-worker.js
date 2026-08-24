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
        requireInitialized();
        sendDelta(player.replaceSceneDeltaJson(message.sceneJson));
        respond(message.requestId, {
          type: "result",
          operation: "replace_scene",
          time: player.time(),
          nextPatchSequence: String(player.nextPatchSequence()),
          sceneJson: player.sceneJson(),
        });
        return;
      case "reconcile_scene": {
        requireInitialized();
        const result = JSON.parse(player.reconcileSceneDeltaJson(message.sceneJson));
        if (result.delta !== null) {
          sendDelta(result.delta);
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
        requireInitialized();
        const delta = player.applyPatchBatchDeltaJson(message.patchBatchJson);
        if (delta !== undefined) {
          sendDelta(delta);
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
      case "stop":
        stopped = true;
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
      onWritable: drainLatestTick,
    });
  }

  const initial = player.initialDeltaJson();
  if (!sendDelta(initial)) {
    throw new Error("execution transport rejected its initial snapshot");
  }
  initialized = true;
  postMain({ type: "ready", transportMode });
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
    drainLatestTick();
    return;
  }
  if (message.type === "transport_writable") {
    drainLatestTick();
    return;
  }
  if (message.type === "render_error") {
    fail(new Error(`render worker failed: ${message.message}`), null);
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

function drainLatestTick() {
  if (!initialized || stopped || latestTick === null || !transportCanSend()) {
    return;
  }
  const timestamp = latestTick;
  latestTick = null;
  try {
    const delta = player.tickDeltaJson(timestamp);
    if (delta !== undefined && !sendDelta(delta)) {
      // Capacity is checked before evaluation, so this can only happen if a
      // concurrent transport consumer changed state between the two operations.
      throw new Error("execution transport became backpressured after frame evaluation");
    }
  } catch (error) {
    fail(error, null);
  }
}

function sendDelta(json) {
  if (json === undefined || json === null) {
    return true;
  }
  const sent = transport.send(json);
  if (sent && transportMode === EXECUTION_TRANSPORT_SHARED) {
    renderPort.postMessage({ type: "shared_delta" });
  }
  return sent;
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
