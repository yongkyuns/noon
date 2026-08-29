import init, { EngineScenePlayer } from "./pkg/noon_web.js";
import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  SharedExecutionDeltaWriter,
  TransferableExecutionDeltaSender,
  createSharedExecutionMailbox,
  executionDeltaMetadata,
} from "./execution-transport.js";

const ENGINE_CHANNEL = "noon.engine";
const ENGINE_PROTOCOL_VERSION = 1;
const HOST_CHANNEL = "noon.host-callback";
const HOST_PROTOCOL_VERSION = 1;

let renderPort = null;
let transportMode = null;
let transport = null;
let player = null;
let latestTick = null;
let viewportAspect = null;
let pendingVisibility = null;
let lastVisibility = null;
let controlQueue = [];
let initialized = false;
let stopped = false;

let hostPort = null;
let hostCallbacks = null;
let hostGeneration = 0;
let hostNextRequestId = 0;
let hostNextSequence = 0;
let hostInFlight = null;
let pendingHostResult = null;
let lastHostPhaseTime = Number.NaN;
let hostFrameObjects = new Map();
let hostFrameOrder = [];
let hostMetrics = freshHostMetrics();

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
      case "set_loop_duration":
      case "pause":
      case "resume":
      case "seek":
      case "restart_playback":
      case "apply_patch":
      case "configure_callbacks":
      case "request_callback_phase":
        enqueueControl(message);
        return;
      case "attach_host_port":
        attachHostPort(message);
        respond(message.requestId, { type: "host_port_attached" });
        return;
      case "state":
        requireInitialized();
        respond(message.requestId, {
          type: "state",
          time: player.time(),
          playing: player.isPlaying(),
          nextPatchSequence: String(player.nextPatchSequence()),
          sceneJson: player.sceneJson(),
        });
        return;
      case "metrics":
        requireInitialized();
        respond(message.requestId, {
          type: "metrics",
          metrics: currentEngineMetrics(),
        });
        return;
      case "stop":
        stopped = true;
        controlQueue = [];
        latestTick = null;
        viewportAspect = null;
        pendingVisibility = null;
        lastVisibility = null;
        clearHostCallbacks();
        hostPort?.close?.();
        hostPort = null;
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
      maxInFlight: 1,
      onWritable: handleTransportWritable,
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
    const requestsVisibility = message.aspect !== undefined && message.aspect !== null;
    if (requestsVisibility && (!Number.isFinite(message.aspect) || message.aspect <= 0)) {
      fail(new Error("render worker sent an invalid viewport aspect"), null);
      return;
    }
    if (hostInFlight !== null && message.timestamp > hostInFlight.presentationTimestamp) {
      hostInFlight.late = true;
      hostMetrics.missedDeadlines += 1;
    }
    if (requestsVisibility) {
      const activatingVisibility = viewportAspect === null;
      viewportAspect = message.aspect;
      if (activatingVisibility) {
        renderPort.postMessage({ type: "visibility_active" });
      }
    }
    latestTick = {
      timestamp: message.timestamp,
      aspect: requestsVisibility ? message.aspect : null,
    };
    drainWork();
    return;
  }
  if (message.type === "transport_writable") {
    handleTransportWritable();
    return;
  }
  if (message.type === "render_error") {
    fail(new Error(`render worker failed: ${message.message}`), null);
  }
}

function handleTransportWritable() {
  if (pendingVisibility !== null) {
    sendVisibilityOrThrow(pendingVisibility);
    lastVisibility = pendingVisibility;
    pendingVisibility = null;
  }
  drainWork();
}

function attachHostPort(message) {
  requireInitialized();
  if (!Number.isSafeInteger(message.requestId) || message.requestId < 0) {
    throw new Error("engine request ID must be a non-negative safe integer");
  }
  if (!(message.port instanceof MessagePort)) {
    throw new Error("engine host attachment requires a MessagePort");
  }
  hostPort?.close?.();
  hostPort = message.port;
  hostPort.addEventListener("message", (event) => handleHostMessage(event.data));
  hostPort.start();
}

function handleHostMessage(message) {
  try {
    validateHostMessage(message);
    if (hostInFlight === null || message.requestId !== hostInFlight.requestId) {
      return;
    }
    if (message.generation !== hostInFlight.generation) {
      return;
    }

    const completed = hostInFlight;
    hostInFlight = null;
    const duration = Math.max(0, performance.now() - completed.startedAt);
    hostMetrics.completed += 1;
    hostMetrics.lastDurationMs = duration;
    hostMetrics.maxDurationMs = Math.max(hostMetrics.maxDurationMs, duration);

    if (message.type === "error") {
      hostMetrics.errors += 1;
      postMain({
        type: "host_callback_error",
        message: String(message.message || "host callback failed"),
      });
      requestLatestHostPhase();
      return;
    }
    if (message.type !== "callback_result" || typeof message.patchBatchJson !== "string") {
      throw new Error(`unknown host callback response ${message.type}`);
    }

    if (completed.late || completed.generation !== hostGeneration) {
      hostMetrics.droppedLateResults += 1;
      requestLatestHostPhase();
      return;
    }

    const batch = JSON.parse(message.patchBatchJson);
    if (!Number.isSafeInteger(batch.sequence) || batch.sequence !== hostNextSequence) {
      throw new Error(
        `host callback returned sequence ${batch.sequence}; expected ${hostNextSequence}`,
      );
    }
    pendingHostResult = {
      generation: completed.generation,
      sequence: hostNextSequence,
      patchBatchJson: message.patchBatchJson,
    };
    drainWork();
  } catch (error) {
    hostMetrics.errors += 1;
    hostInFlight = null;
    pendingHostResult = null;
    postMain({ type: "host_callback_error", message: String(error?.message ?? error) });
    drainWork();
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

  if (controlQueue.length > 0) {
    return;
  }

  if (pendingHostResult !== null && transportCanSend()) {
    try {
      commitPendingHostResult();
    } catch (error) {
      hostMetrics.errors += 1;
      postMain({ type: "host_callback_error", message: String(error?.message ?? error) });
      pendingHostResult = null;
    }
  }

  if (pendingHostResult !== null || latestTick === null || !transportCanSend()) {
    return;
  }

  const tick = latestTick;
  latestTick = null;
  if (tick.aspect !== null) {
    viewportAspect = tick.aspect;
  }
  try {
    const delta = player.tickDeltaJson(tick.timestamp);
    if (delta !== undefined && delta !== null) {
      sendDeltaOrThrow(delta);
    } else if (tick.aspect === null) {
      // Ordinary execution ticks do not participate in viewport visibility culling.
    } else if (lastVisibility !== null && Object.is(lastVisibility.aspect, viewportAspect)) {
      sendVisibilityOrThrow(lastVisibility);
    } else {
      // The mirror must advance to the same semantic frame before accepting a new
      // visibility envelope. A complete snapshot is needed only for first activation
      // or an aspect change when evaluation produced no object delta.
      sendDeltaOrThrow(player.snapshotDeltaJson());
    }
    maybeRequestHostCallback(tick.timestamp);
  } catch (error) {
    fail(error, null);
  }
}

function executeControl(message) {
  switch (message.type) {
    case "replace_scene": {
      validateOptionalLoopDuration(message.loopDurationSeconds);
      clearHostCallbacks();
      const delta = player.replaceSceneDeltaJson(message.sceneJson);
      applyOptionalLoopDuration(message.loopDurationSeconds);
      latestTick = null;
      sendDeltaOrThrow(delta);
      respond(message.requestId, runtimeResult("replace_scene"));
      return;
    }
    case "reconcile_scene": {
      validateOptionalLoopDuration(message.loopDurationSeconds);
      clearHostCallbacks();
      const result = JSON.parse(player.reconcileSceneDeltaJson(message.sceneJson));
      applyOptionalLoopDuration(message.loopDurationSeconds);
      if (result.delta !== null && result.delta !== undefined) {
        sendDeltaOrThrow(result.delta);
      }
      respond(message.requestId, {
        ...runtimeResult("reconcile_scene"),
        incremental: result.incremental,
      });
      return;
    }
    case "set_loop_duration": {
      validateRequiredLoopDuration(message.loopDurationSeconds);
      player.setLoopDurationSeconds(message.loopDurationSeconds);
      respond(message.requestId, runtimeResult("set_loop_duration"));
      return;
    }
    case "pause": {
      beginPlaybackControl(false);
      player.pause();
      respond(message.requestId, runtimeResult("pause"));
      return;
    }
    case "resume": {
      beginPlaybackControl(false);
      player.resume();
      respond(message.requestId, runtimeResult("resume"));
      return;
    }
    case "seek": {
      beginPlaybackControl(true);
      const delta = player.seekDeltaJson(message.time);
      if (delta !== null && delta !== undefined) {
        sendDeltaOrThrow(delta);
      }
      requestLatestHostPhase();
      respond(message.requestId, runtimeResult("seek"));
      return;
    }
    case "restart_playback": {
      beginPlaybackControl(true);
      const delta = player.seekDeltaJson(0);
      if (delta !== null && delta !== undefined) {
        sendDeltaOrThrow(delta);
      }
      requestLatestHostPhase();
      respond(message.requestId, runtimeResult("restart_playback"));
      return;
    }
    case "apply_patch": {
      const delta = player.applyPatchBatchDeltaJson(message.patchBatchJson);
      if (delta !== undefined && delta !== null) {
        sendDeltaOrThrow(delta);
      }
      respond(message.requestId, runtimeResult("apply_patch"));
      return;
    }
    case "configure_callbacks": {
      configureCallbacks(message.callbacks);
      respond(message.requestId, {
        type: "callbacks_configured",
        enabled: hostCallbacks !== null,
        generation: hostGeneration,
      });
      return;
    }
    case "request_callback_phase": {
      if (hostCallbacks === null || hostPort === null) {
        throw new Error("callback phase synchronization requires configured host callbacks");
      }
      requestLatestHostPhase();
      respond(message.requestId, {
        type: "callback_phase_requested",
        generation: hostGeneration,
        hostRequestId: hostInFlight?.requestId ?? null,
      });
      return;
    }
    default:
      throw new Error(`unknown queued engine command ${message.type}`);
  }
}

function beginPlaybackControl(invalidateHostPhase) {
  latestTick = null;
  if (!invalidateHostPhase || hostCallbacks === null) {
    return;
  }
  hostGeneration = checkedIncrement(hostGeneration, "host callback generation");
  if (hostInFlight !== null || pendingHostResult !== null) {
    hostMetrics.droppedLateResults += 1;
  }
  hostInFlight = null;
  pendingHostResult = null;
  lastHostPhaseTime = Number.NaN;
}

function validateRequiredLoopDuration(duration) {
  if (!Number.isFinite(duration) || duration <= 0) {
    throw new Error("execution engine loop duration must be positive and finite");
  }
}

function validateOptionalLoopDuration(duration) {
  if (duration === null || duration === undefined) {
    return;
  }
  validateRequiredLoopDuration(duration);
}

function applyOptionalLoopDuration(duration) {
  if (duration !== null && duration !== undefined) {
    player.setLoopDurationSeconds(duration);
  }
}

function runtimeResult(operation) {
  return {
    type: "result",
    operation,
    time: player.time(),
    playing: player.isPlaying(),
    nextPatchSequence: String(player.nextPatchSequence()),
    sceneJson: player.sceneJson(),
  };
}

function configureCallbacks(callbacks) {
  clearHostCallbacks();
  if (callbacks === null || callbacks === undefined) {
    return;
  }
  if (hostPort === null) {
    throw new Error("host callbacks require an attached Python host port");
  }
  validateCallbackConfig(callbacks);
  hostCallbacks = callbacks;
  hostMetrics = freshHostMetrics();
  lastHostPhaseTime = Number.NaN;
  const snapshot = player.snapshotDeltaJson();
  sendDeltaOrThrow(snapshot);
}

function clearHostCallbacks() {
  hostGeneration = checkedIncrement(hostGeneration, "host callback generation");
  hostCallbacks = null;
  hostNextSequence = 0;
  hostInFlight = null;
  pendingHostResult = null;
  lastHostPhaseTime = Number.NaN;
  hostFrameObjects.clear();
  hostFrameOrder = [];
}

function maybeRequestHostCallback(presentationTimestamp) {
  if (
    hostCallbacks === null ||
    hostPort === null ||
    hostInFlight !== null ||
    pendingHostResult !== null
  ) {
    return;
  }
  const time = player.time();
  if (Object.is(time, lastHostPhaseTime)) {
    return;
  }
  requestHostCallback(presentationTimestamp, time);
}

function requestLatestHostPhase() {
  if (hostCallbacks === null || hostPort === null || pendingHostResult !== null) {
    return;
  }
  const time = player.time();
  if (Object.is(time, lastHostPhaseTime)) {
    return;
  }
  requestHostCallback(performance.now(), time);
}

function requestHostCallback(presentationTimestamp, time) {
  const requestId = hostNextRequestId;
  hostNextRequestId = checkedIncrement(hostNextRequestId, "host callback request ID");
  const frame = buildHostFrame(time);
  lastHostPhaseTime = time;
  hostInFlight = {
    requestId,
    generation: hostGeneration,
    presentationTimestamp,
    startedAt: performance.now(),
    late: false,
  };
  hostMetrics.requests += 1;
  hostPort.postMessage({
    channel: HOST_CHANNEL,
    protocolVersion: HOST_PROTOCOL_VERSION,
    type: "callback_phase",
    requestId,
    generation: hostGeneration,
    sessionId: hostCallbacks.session_id,
    sequence: hostNextSequence,
    frame,
  });
}

function buildHostFrame(time) {
  const objects = [];
  const objectIndices = new Map();
  const invocations = [];
  for (const slot of hostCallbacks.slots) {
    if (!callbackSlotActiveAt(slot, time)) {
      continue;
    }
    const indices = [];
    for (const objectId of slot.objects) {
      let index = objectIndices.get(objectId);
      if (index === undefined) {
        const state = hostFrameObjects.get(objectId);
        if (state === undefined) {
          throw new Error(`host callback snapshot is missing object ${objectId}`);
        }
        index = objects.length;
        objectIndices.set(objectId, index);
        objects.push({
          object: objectId,
          transform: state.transform,
          style: state.style,
          presence: state.presence,
          appearance: state.appearance,
          reveal: state.reveal,
          morph: state.morph,
        });
      }
      indices.push(index);
    }
    invocations.push({ callback: slot.id, object_indices: indices });
  }
  const previous = Number.isFinite(hostMetrics.lastFrameTime) ? hostMetrics.lastFrameTime : time;
  const frame = {
    time,
    delta_time: time - previous,
    objects,
    invocations,
  };
  hostMetrics.lastFrameTime = time;
  return frame;
}

function commitPendingHostResult() {
  const result = pendingHostResult;
  if (result === null) {
    return;
  }
  pendingHostResult = null;
  if (result.generation !== hostGeneration || hostCallbacks === null) {
    return;
  }
  if (result.sequence !== hostNextSequence) {
    throw new Error(
      `pending host callback sequence ${result.sequence}; expected ${hostNextSequence}`,
    );
  }
  const delta = player.applyHostPatchBatchDeltaJson(result.patchBatchJson);
  if (delta !== undefined && delta !== null) {
    sendDeltaOrThrow(delta);
  }
  hostNextSequence = checkedIncrement(hostNextSequence, "host callback patch sequence");
  hostMetrics.committed += 1;
}

function applyHostMirror(delta) {
  if (delta.snapshot) {
    hostFrameObjects.clear();
    hostFrameOrder = delta.objects
      .slice()
      .sort((left, right) => left.order - right.order)
      .map((object) => object.object);
  }
  for (const object of delta.objects) {
    hostFrameObjects.set(object.object, object);
  }
}

function transportCanSend() {
  if (transport === null || pendingVisibility !== null) {
    return false;
  }
  if (typeof transport.canSend === "function") {
    return transport.canSend();
  }
  if (typeof transport.inFlight === "function") {
    return transport.inFlight() < 1;
  }
  return true;
}

function sendDeltaOrThrow(json) {
  if (json === undefined || json === null) {
    return;
  }
  const metadata = executionDeltaMetadata(json);
  const decoded = hostCallbacks === null ? null : JSON.parse(json);
  if (!transport.send(json)) {
    throw new Error("execution transport became backpressured after work was admitted");
  }
  if (decoded !== null) {
    applyHostMirror(decoded);
  }
  if (transportMode === EXECUTION_TRANSPORT_SHARED) {
    renderPort.postMessage({ type: "shared_delta" });
  }
  if (viewportAspect !== null) {
    pendingVisibility = {
      session: metadata.session,
      sequence: metadata.sequence,
      aspect: viewportAspect,
      json: player.viewportVisibilityJson(viewportAspect),
    };
  }
}

function sendVisibilityOrThrow(visibility) {
  if (visibility === null) {
    return;
  }
  renderPort.postMessage({
    type: "visibility",
    session: visibility.session,
    sequence: visibility.sequence,
    json: visibility.json,
  });
}

function currentEngineMetrics() {
  return {
    visibility: {
      active: viewportAspect !== null,
      aspect: viewportAspect,
      pending: pendingVisibility !== null,
      lastSession: lastVisibility?.session ?? null,
      lastSequence: lastVisibility?.sequence ?? null,
    },
    host: {
      enabled: hostCallbacks !== null,
      inFlight: hostInFlight !== null,
      pendingCommit: pendingHostResult !== null,
      generation: hostGeneration,
      nextSequence: hostNextSequence,
      requests: hostMetrics.requests,
      completed: hostMetrics.completed,
      committed: hostMetrics.committed,
      missedDeadlines: hostMetrics.missedDeadlines,
      droppedLateResults: hostMetrics.droppedLateResults,
      errors: hostMetrics.errors,
      lastDurationMs: hostMetrics.lastDurationMs,
      maxDurationMs: hostMetrics.maxDurationMs,
      lastFrameTime: hostMetrics.lastFrameTime,
    },
  };
}

function freshHostMetrics() {
  return {
    requests: 0,
    completed: 0,
    committed: 0,
    missedDeadlines: 0,
    droppedLateResults: 0,
    errors: 0,
    lastDurationMs: null,
    maxDurationMs: 0,
    lastFrameTime: null,
  };
}

function callbackSlotActiveAt(slot, time) {
  if (slot.active_after !== undefined && slot.active_after !== null && time < slot.active_after) {
    return false;
  }
  if (slot.active_through !== undefined && slot.active_through !== null && time >= slot.active_through) {
    return false;
  }
  return true;
}

function validateCallbackConfig(callbacks) {
  if (!callbacks || typeof callbacks !== "object") {
    throw new Error("host callback configuration must be an object");
  }
  if (!Number.isSafeInteger(callbacks.session_id) || callbacks.session_id < 0) {
    throw new Error("host callback configuration has an invalid session ID");
  }
  if (!Array.isArray(callbacks.slots) || callbacks.slots.length === 0) {
    throw new Error("host callback configuration requires callback slots");
  }
  for (const slot of callbacks.slots) {
    if (!slot || !Number.isSafeInteger(slot.id) || slot.id < 0 || !Array.isArray(slot.objects)) {
      throw new Error("host callback configuration contains an invalid slot");
    }
    for (const object of slot.objects) {
      if (!Number.isSafeInteger(object) || object < 0) {
        throw new Error("host callback slot contains an invalid object ID");
      }
    }
    for (const field of ["active_after", "active_through"]) {
      const value = slot[field];
      if (value !== undefined && value !== null && (!Number.isFinite(value) || value < 0)) {
        throw new Error(`host callback slot contains invalid ${field}`);
      }
    }
    if (
      slot.active_after !== undefined &&
      slot.active_after !== null &&
      slot.active_through !== undefined &&
      slot.active_through !== null &&
      slot.active_through < slot.active_after
    ) {
      throw new Error("host callback slot has an invalid activation window");
    }
  }
}

function validateHostMessage(message) {
  if (
    !message ||
    typeof message !== "object" ||
    message.channel !== HOST_CHANNEL ||
    message.protocolVersion !== HOST_PROTOCOL_VERSION ||
    !Number.isSafeInteger(message.requestId) ||
    message.requestId < 0 ||
    !Number.isSafeInteger(message.generation) ||
    message.generation < 0
  ) {
    throw new Error("invalid host callback response envelope");
  }
}

function checkedIncrement(value, label) {
  if (!Number.isSafeInteger(value) || value < 0 || value >= Number.MAX_SAFE_INTEGER) {
    throw new Error(`${label} space exhausted`);
  }
  return value + 1;
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
