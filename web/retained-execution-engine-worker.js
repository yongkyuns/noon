import init, { MixedRetainedEngineScenePlayer } from "./pkg/noon_web.js";
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
let resourceBundleBytes = 0;

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
      case "set_loop_duration":
        requireInitialized();
        validateLoopDuration(message.loopDurationSeconds);
        player.setLoopDurationSeconds(message.loopDurationSeconds);
        respond(message.requestId, runtimeResult("set_loop_duration"));
        return;
      case "state":
        requireInitialized();
        respond(message.requestId, {
          type: "state",
          time: player.time(),
          nextPatchSequence: "0",
          sceneJson: player.legacySceneJson(),
          retainedDocumentJson: player.retainedDocumentJson(),
        });
        return;
      case "metrics":
        requireInitialized();
        respond(message.requestId, {
          type: "metrics",
          metrics: {
            retained: true,
            mixed: true,
            time: player.time(),
            resourceBundleBytes,
            resourceBundleTransfers: 1,
          },
        });
        return;
      case "replace_scene":
      case "reconcile_scene":
      case "apply_patch":
      case "configure_callbacks":
      case "attach_host_port":
        throw new Error(
          `${message.type} is not supported by mixed retained execution yet; rebuild the scene instead`,
        );
      case "stop":
        stopped = true;
        latestTick = null;
        renderPort?.close?.();
        self.close();
        return;
      default:
        throw new Error(`unknown mixed retained engine command ${message.type}`);
    }
  } catch (error) {
    fail(error, message?.requestId ?? null);
  }
}

async function initialize(message) {
  if (initialized) {
    throw new Error("mixed retained execution engine worker is already initialized");
  }
  if (!(message.port instanceof MessagePort)) {
    throw new Error("mixed retained execution engine init requires a render MessagePort");
  }
  if (typeof message.sceneJson !== "string" || message.sceneJson.trim() === "") {
    throw new Error("mixed retained execution engine init requires legacy scene JSON");
  }
  if (
    typeof message.retainedDocumentJson !== "string" ||
    message.retainedDocumentJson.trim() === ""
  ) {
    throw new Error("mixed retained execution engine init requires retained document JSON");
  }
  validateLoopDuration(message.loopDurationSeconds);
  if (!Number.isSafeInteger(message.session) || message.session < 0) {
    throw new Error("mixed retained execution session must be a non-negative safe integer");
  }
  if (
    message.transportMode !== EXECUTION_TRANSPORT_SHARED &&
    message.transportMode !== EXECUTION_TRANSPORT_TRANSFERABLE
  ) {
    throw new Error(`unsupported mixed retained execution transport mode ${message.transportMode}`);
  }

  renderPort = message.port;
  renderPort.addEventListener("message", (event) => handleRenderMessage(event.data));
  renderPort.start();
  transportMode = message.transportMode;
  await init();
  player = new MixedRetainedEngineScenePlayer(
    message.sceneJson,
    message.retainedDocumentJson,
    message.loopDurationSeconds,
    message.session,
  );

  // wasm-bindgen returns a view/copy for Vec<u8>; make an independently owned
  // transferable buffer so detaching it cannot affect WebAssembly memory.
  const resources = Uint8Array.from(player.resourceBundleBytes());
  resourceBundleBytes = resources.byteLength;
  renderPort.postMessage(
    { type: "retained_resources", bytes: resources },
    [resources.buffer],
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

  sendDeltaOrThrow(player.initialDeltaJson());
  initialized = true;
  postMain({ type: "ready", transportMode, retained: true, mixed: true });
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
    fail(new Error(`mixed retained render worker failed: ${message.message}`), null);
  }
}

function drainWork() {
  if (!initialized || stopped || player === null || transport === null) {
    return;
  }
  if (latestTick === null || !transportCanSend()) {
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
    throw new Error("mixed retained execution transport became backpressured after work was admitted");
  }
  if (transportMode === EXECUTION_TRANSPORT_SHARED) {
    renderPort.postMessage({ type: "shared_delta" });
  }
}

function runtimeResult(operation) {
  return {
    type: "result",
    operation,
    time: player.time(),
    nextPatchSequence: "0",
    sceneJson: player.legacySceneJson(),
    retainedDocumentJson: player.retainedDocumentJson(),
  };
}

function respond(requestId, payload) {
  if (!Number.isSafeInteger(requestId) || requestId < 0) {
    throw new Error("mixed retained engine request ID must be a non-negative safe integer");
  }
  postMain({ requestId, ...payload });
}

function fail(error, requestId) {
  stopped = true;
  const message = String(error?.message ?? error);
  renderPort?.postMessage({ type: "render_error", message });
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
    throw new Error("invalid mixed retained execution engine control envelope");
  }
}

function validateLoopDuration(duration) {
  if (!Number.isFinite(duration) || duration <= 0) {
    throw new Error("mixed retained execution loop duration must be positive and finite");
  }
}

function requireInitialized() {
  if (!initialized || player === null) {
    throw new Error("mixed retained execution engine worker is not initialized");
  }
}
