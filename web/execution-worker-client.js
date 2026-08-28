import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  selectExecutionTransportMode,
} from "./execution-transport.js";
import { replaceExecutionCanvas } from "./execution-canvas.js";
import { projectLegacyReactiveSceneJson } from "./legacy-reactive-projection.js";

const ENGINE_CHANNEL = "noon.engine";
const ENGINE_PROTOCOL_VERSION = 1;
const RENDER_CHANNEL = "noon.render";
const RENDER_PROTOCOL_VERSION = 1;
const WORKER_OWNERS = Object.freeze(["engine", "render"]);
const DEFAULT_SHARED_SLOT_CAPACITY = 1024 * 1024;

export class ExecutionWorkerClient {
  #canvas;
  #engineWorker = null;
  #renderWorker = null;
  #nextRequestIds = { engine: 0, render: 0 };
  #pending = new Map();
  #session = 0;
  #sceneJson = null;
  #loopDurationSeconds = 4;
  #transportMode = null;
  #sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY;
  #ready = null;
  #playing = true;
  #onError;
  #onRecoverableError;
  #hostAuthoringClient = null;
  #hostCallbacks = null;
  #fatalOwner = null;
  #staleWorkerEvents = { engine: 0, render: 0 };
  #staleResponses = { engine: 0, render: 0 };

  constructor(canvas, { onError = null, onRecoverableError = null } = {}) {
    if (!(canvas instanceof HTMLCanvasElement)) {
      throw new TypeError("ExecutionWorkerClient requires an HTMLCanvasElement");
    }
    if (onError !== null && typeof onError !== "function") {
      throw new TypeError("ExecutionWorkerClient onError must be a function");
    }
    if (onRecoverableError !== null && typeof onRecoverableError !== "function") {
      throw new TypeError("ExecutionWorkerClient onRecoverableError must be a function");
    }
    this.#canvas = canvas;
    this.#onError = onError;
    this.#onRecoverableError = onRecoverableError;
  }

  get canvas() {
    return this.#canvas;
  }

  get transportMode() {
    return this.#transportMode;
  }

  get diagnostics() {
    return Object.freeze({
      session: this.#session,
      engine: this.#ownerDiagnostics("engine"),
      render: this.#ownerDiagnostics("render"),
    });
  }

  async start(
    sceneJson,
    {
      loopDurationSeconds = 4,
      transportMode = selectExecutionTransportMode(),
      sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY,
    } = {},
  ) {
    if (this.#engineWorker !== null || this.#renderWorker !== null) {
      throw new Error("ExecutionWorkerClient is already started");
    }
    validateSceneJson(sceneJson);
    sceneJson = projectLegacyReactiveSceneJson(sceneJson);
    validateLoopDurationSeconds(loopDurationSeconds);
    if (
      transportMode !== EXECUTION_TRANSPORT_SHARED &&
      transportMode !== EXECUTION_TRANSPORT_TRANSFERABLE
    ) {
      throw new TypeError(`unsupported execution transport mode ${transportMode}`);
    }
    if (
      transportMode === EXECUTION_TRANSPORT_SHARED &&
      selectExecutionTransportMode() !== EXECUTION_TRANSPORT_SHARED
    ) {
      throw new Error("shared execution transport requires cross-origin isolation");
    }
    if (typeof this.#canvas.transferControlToOffscreen !== "function") {
      throw new Error("OffscreenCanvas transfer is unavailable in this browser");
    }

    this.#sceneJson = sceneJson;
    this.#loopDurationSeconds = loopDurationSeconds;
    this.#transportMode = transportMode;
    this.#sharedSlotCapacity = sharedSlotCapacity;
    this.#session = checkedNextSession(this.#session);

    // The OffscreenCanvas inherits the HTML canvas backing-store dimensions at
    // transfer time. Size that backing store from the already-laid-out CSS box
    // before handing control to the render worker; otherwise Chrome/WebGL can
    // create its first surface at the HTML default 300×150 and only correct it
    // after renderer startup.
    const devicePixelRatio = window.devicePixelRatio || 1;
    const initialWidth = Math.max(1, Math.round(this.#canvas.clientWidth * devicePixelRatio));
    const initialHeight = Math.max(1, Math.round(this.#canvas.clientHeight * devicePixelRatio));
    this.#canvas.width = initialWidth;
    this.#canvas.height = initialHeight;

    let canvasTransferred = false;
    try {
      const channel = new MessageChannel();
      const offscreen = this.#canvas.transferControlToOffscreen();
      canvasTransferred = true;
      this.#engineWorker = new Worker(new URL("./execution-engine-worker.js", import.meta.url), {
        type: "module",
        name: "noon-engine",
      });
      this.#renderWorker = new Worker(new URL("./execution-render-worker.js", import.meta.url), {
        type: "module",
        name: "noon-render",
      });

      const engineReady = this.#workerReady(this.#engineWorker, ENGINE_CHANNEL, "engine");
      const renderReady = this.#workerReady(this.#renderWorker, RENDER_CHANNEL, "render");
      this.#ready = Promise.all([engineReady, renderReady]).then(([engine, render]) => ({
        engine,
        render,
        transportMode,
        session: this.#session,
      }));

      this.#renderWorker.postMessage(
        renderEnvelope("init", {
          canvas: offscreen,
          port: channel.port2,
          transportMode,
          width: initialWidth,
          height: initialHeight,
        }),
        [offscreen, channel.port2],
      );
      this.#engineWorker.postMessage(
        engineEnvelope("init", {
          port: channel.port1,
          sceneJson,
          loopDurationSeconds,
          transportMode,
          sharedSlotCapacity,
          session: this.#session,
        }),
        [channel.port1],
      );
      const ready = await this.#ready;
      this.#playing = true;
      this.#fatalOwner = null;
      return ready;
    } catch (error) {
      this.#rollbackFailedStart(error, canvasTransferred);
      throw error;
    }
  }

  ready() {
    if (this.#ready === null) {
      throw new Error("ExecutionWorkerClient has not been started");
    }
    return this.#ready;
  }

  async replaceScene(
    sceneJson,
    { callbacks = null, authoringClient = null, loopDurationSeconds = null } = {},
  ) {
    validateSceneJson(sceneJson);
    sceneJson = projectLegacyReactiveSceneJson(sceneJson);
    const duration = validateOptionalLoopDurationSeconds(loopDurationSeconds);
    const result = await this.#requestEngine("replace_scene", {
      sceneJson,
      loopDurationSeconds: duration,
    });
    this.#rememberPlaying(result);
    this.#sceneJson = result.sceneJson ?? sceneJson;
    if (duration !== null) {
      this.#loopDurationSeconds = duration;
    }
    await this.configureHostCallbacks(callbacks, authoringClient);
    return result;
  }

  async reconcileScene(
    sceneJson,
    { callbacks = null, authoringClient = null, loopDurationSeconds = null } = {},
  ) {
    validateSceneJson(sceneJson);
    sceneJson = projectLegacyReactiveSceneJson(sceneJson);
    const duration = validateOptionalLoopDurationSeconds(loopDurationSeconds);
    const result = await this.#requestEngine("reconcile_scene", {
      sceneJson,
      loopDurationSeconds: duration,
    });
    this.#rememberPlaying(result);
    this.#sceneJson = result.sceneJson ?? sceneJson;
    if (duration !== null) {
      this.#loopDurationSeconds = duration;
    }
    await this.configureHostCallbacks(callbacks, authoringClient);
    return result;
  }

  async setLoopDurationSeconds(loopDurationSeconds) {
    const duration = validateLoopDurationSeconds(loopDurationSeconds);
    const result = await this.#requestEngine("set_loop_duration", {
      loopDurationSeconds: duration,
    });
    this.#rememberPlaying(result);
    this.#loopDurationSeconds = duration;
    return result;
  }

  async pause() {
    const result = await this.#requestEngine("pause", {});
    this.#rememberPlaying(result);
    return result;
  }

  async resume() {
    const result = await this.#requestEngine("resume", {});
    this.#rememberPlaying(result);
    return result;
  }

  async seek(timeSeconds) {
    const time = validateSeekTimeSeconds(timeSeconds, this.#loopDurationSeconds);
    const result = await this.#requestEngine("seek", { time });
    this.#rememberPlaying(result);
    return result;
  }

  async restartPlayback() {
    const result = await this.#requestEngine("restart_playback", {});
    this.#rememberPlaying(result);
    return result;
  }

  async applyPatchBatch(patchBatchJson) {
    if (typeof patchBatchJson !== "string" || patchBatchJson.trim() === "") {
      throw new TypeError("patch batch must be non-empty JSON text");
    }
    const result = await this.#requestEngine("apply_patch", { patchBatchJson });
    this.#rememberPlaying(result);
    if (typeof result.sceneJson === "string") {
      this.#sceneJson = result.sceneJson;
    }
    return result;
  }

  async configureHostCallbacks(callbacks, authoringClient = null) {
    await this.ready();
    if (callbacks === null || callbacks === undefined) {
      this.#hostCallbacks = null;
      await this.#requestEngine("configure_callbacks", { callbacks: null });
      return;
    }
    validateCallbacks(callbacks);
    if (!authoringClient || typeof authoringClient.attachEnginePort !== "function") {
      throw new TypeError("host callbacks require a PythonAuthoringClient");
    }
    if (this.#hostAuthoringClient !== authoringClient) {
      const channel = new MessageChannel();
      await authoringClient.attachEnginePort(channel.port2);
      await this.#requestEngine(
        "attach_host_port",
        { port: channel.port1 },
        [channel.port1],
      );
      this.#hostAuthoringClient = authoringClient;
    }
    this.#hostCallbacks = cloneCallbacks(callbacks);
    await this.#requestEngine("configure_callbacks", { callbacks: this.#hostCallbacks });
  }

  async state() {
    return this.#requestEngine("state", {});
  }

  async metrics() {
    const [render, engine] = await Promise.all([
      this.#requestRender("metrics", {}),
      this.#requestEngine("metrics", {}),
    ]);
    return { ...render, engineMetrics: engine.metrics };
  }

  resize(width, height, devicePixelRatio = 1) {
    this.#requireStarted();
    if (!Number.isFinite(width) || !Number.isFinite(height) || !Number.isFinite(devicePixelRatio)) {
      throw new TypeError("execution canvas dimensions must be finite");
    }
    const physicalWidth = Math.max(1, Math.round(width * devicePixelRatio));
    const physicalHeight = Math.max(1, Math.round(height * devicePixelRatio));
    // Once control has moved to OffscreenCanvas, HTMLCanvasElement bitmap sizing
    // belongs to the render worker. Writing width/height here throws InvalidStateError.
    this.#renderWorker.postMessage(
      renderEnvelope("resize", { width: physicalWidth, height: physicalHeight }),
    );
  }

  async restart({ failedOwner = this.#fatalOwner } = {}) {
    if (this.#sceneJson === null || this.#transportMode === null) {
      throw new Error("ExecutionWorkerClient has not been started");
    }
    if (failedOwner !== null && failedOwner !== "engine" && failedOwner !== "render") {
      throw new TypeError(`unsupported failed execution owner ${failedOwner}`);
    }
    if (failedOwner === "engine" && this.#renderWorker !== null) {
      return this.#restartEngine();
    }
    return this.#restartAll();
  }

  async #restartEngine() {
    const wasPlaying = this.#playing;
    const callbacks = this.#hostCallbacks;
    const authoringClient = this.#hostAuthoringClient;
    const reconnectError = new Error("execution engine worker restarting");
    this.#engineWorker?.terminate();
    this.#engineWorker = null;
    this.#rejectOwner("engine", reconnectError);

    const channel = new MessageChannel();
    const renderAttached = this.#request(
      this.#renderWorker,
      "render",
      renderEnvelope,
      "attach_engine",
      { port: channel.port2, transportMode: this.#transportMode },
      [channel.port2],
    ).catch((error) => {
      this.#markFatalOwner("render");
      throw error;
    });
    this.#session = checkedNextSession(this.#session);
    this.#engineWorker = new Worker(new URL("./execution-engine-worker.js", import.meta.url), {
      type: "module",
      name: "noon-engine",
    });
    const engineReady = this.#workerReady(this.#engineWorker, ENGINE_CHANNEL, "engine");
    const nextReady = Promise.all([engineReady, renderAttached]).then(([engine, render]) => ({
      engine,
      render,
      transportMode: this.#transportMode,
      session: this.#session,
    }));
    this.#ready = nextReady;
    this.#engineWorker.postMessage(
      engineEnvelope("init", {
        port: channel.port1,
        sceneJson: this.#sceneJson,
        loopDurationSeconds: this.#loopDurationSeconds,
        transportMode: this.#transportMode,
        sharedSlotCapacity: this.#sharedSlotCapacity,
        session: this.#session,
      }),
      [channel.port1],
    );

    const ready = await nextReady;
    this.#playing = true;
    if (!wasPlaying) {
      await this.pause();
    }
    if (callbacks !== null && authoringClient !== null) {
      this.#hostAuthoringClient = null;
      await this.configureHostCallbacks(callbacks, authoringClient);
      await this.#requestEngine("request_callback_phase", {});
    }
    this.#fatalOwner = null;
    return ready;
  }

  async #restartAll() {
    const sceneJson = this.#sceneJson;
    const loopDurationSeconds = this.#loopDurationSeconds;
    const transportMode = this.#transportMode;
    const sharedSlotCapacity = this.#sharedSlotCapacity;
    const wasPlaying = this.#playing;
    const callbacks = this.#hostCallbacks;
    const authoringClient = this.#hostAuthoringClient;
    if (this.#engineWorker !== null || this.#renderWorker !== null) {
      this.terminate({ preserveHostConfiguration: true });
      this.#canvas = replaceExecutionCanvas(this.#canvas);
    }
    const ready = await this.start(sceneJson, {
      loopDurationSeconds,
      transportMode,
      sharedSlotCapacity,
    });
    if (!wasPlaying) {
      await this.pause();
    }
    if (callbacks !== null && authoringClient !== null) {
      this.#hostAuthoringClient = null;
      await this.configureHostCallbacks(callbacks, authoringClient);
    }
    this.#fatalOwner = null;
    return ready;
  }

  terminate({ preserveHostConfiguration = false } = {}) {
    this.#engineWorker?.terminate();
    this.#renderWorker?.terminate();
    this.#engineWorker = null;
    this.#renderWorker = null;
    this.#ready = null;
    const error = new Error("execution worker client terminated");
    for (const pending of this.#pending.values()) {
      pending.reject(error);
    }
    this.#pending.clear();
    if (!preserveHostConfiguration) {
      this.#hostAuthoringClient = null;
      this.#hostCallbacks = null;
    }
    this.#fatalOwner = null;
  }

  async #requestEngine(type, payload, transfer = []) {
    await this.ready();
    return this.#request(
      this.#engineWorker,
      "engine",
      engineEnvelope,
      type,
      payload,
      transfer,
    );
  }

  async #requestRender(type, payload, transfer = []) {
    await this.ready();
    return this.#request(
      this.#renderWorker,
      "render",
      renderEnvelope,
      type,
      payload,
      transfer,
    );
  }

  #request(worker, owner, envelopeFactory, type, payload, transfer = []) {
    validateWorkerOwner(owner);
    const requestId = this.#nextRequestIds[owner];
    this.#nextRequestIds[owner] = checkedNextRequestId(requestId);
    const result = new Promise((resolve, reject) => {
      this.#pending.set(`${owner}:${requestId}`, { resolve, reject });
    });
    worker.postMessage(envelopeFactory(type, { requestId, ...payload }), transfer);
    return result;
  }

  #workerReady(worker, channel, owner) {
    validateWorkerOwner(owner);
    return new Promise((resolve, reject) => {
      const onMessage = (event) => {
        if (!this.#isCurrentWorker(owner, worker)) {
          this.#recordStaleWorkerEvent(owner);
          return;
        }
        const message = event.data;
        try {
          validateWorkerEnvelope(message, channel);
          if (message.type === "ready") {
            resolve(message);
            return;
          }
          if (message.type === "host_callback_error") {
            this.#notifyRecoverableError(
              new Error(message.message || "host callback failed"),
              "host",
            );
            return;
          }
          if (message.type === "error") {
            const error = new Error(message.message || `${owner} worker failed`);
            if (message.requestId === null || message.requestId === undefined) {
              reject(error);
              this.#rejectOwner(owner, error);
              this.#notifyError(error, owner);
              return;
            }
            this.#settle(owner, message.requestId, ({ reject: rejectPending }) => {
              rejectPending(error);
            });
            return;
          }
          if (message.requestId !== undefined) {
            this.#settle(owner, message.requestId, ({ resolve: resolvePending }) => {
              resolvePending(message);
            });
          }
        } catch (error) {
          reject(error);
          this.#rejectOwner(owner, error);
          this.#notifyError(error, owner);
        }
      };
      worker.addEventListener("message", onMessage);
      worker.addEventListener("error", (event) => {
        if (!this.#isCurrentWorker(owner, worker)) {
          this.#recordStaleWorkerEvent(owner);
          return;
        }
        const error = new Error(event.message || `${owner} worker crashed`);
        reject(error);
        this.#rejectOwner(owner, error);
        this.#notifyError(error, owner);
      });
      worker.addEventListener("messageerror", () => {
        if (!this.#isCurrentWorker(owner, worker)) {
          this.#recordStaleWorkerEvent(owner);
          return;
        }
        const error = new Error(`${owner} worker message could not be decoded`);
        reject(error);
        this.#rejectOwner(owner, error);
        this.#notifyError(error, owner);
      });
    });
  }

  #settle(owner, requestId, settle) {
    validateWorkerOwner(owner);
    if (!Number.isSafeInteger(requestId) || requestId < 0) {
      throw new Error(`${owner} worker returned an invalid request ID`);
    }
    const key = `${owner}:${requestId}`;
    const pending = this.#pending.get(key);
    if (!pending) {
      if (requestId < this.#nextRequestIds[owner]) {
        this.#staleResponses[owner] += 1;
        return false;
      }
      throw new Error(`${owner} worker returned unissued request ID ${requestId}`);
    }
    this.#pending.delete(key);
    settle(pending);
    return true;
  }

  #rejectOwner(owner, error) {
    for (const [key, pending] of this.#pending.entries()) {
      if (key.startsWith(`${owner}:`)) {
        pending.reject(error);
        this.#pending.delete(key);
      }
    }
  }

  #rollbackFailedStart(error, replaceCanvas) {
    this.#engineWorker?.terminate();
    this.#renderWorker?.terminate();
    this.#engineWorker = null;
    this.#renderWorker = null;
    this.#ready = null;
    for (const pending of this.#pending.values()) {
      pending.reject(error);
    }
    this.#pending.clear();
    this.#fatalOwner = null;
    if (replaceCanvas) {
      this.#canvas = replaceExecutionCanvas(this.#canvas);
    }
  }

  #isCurrentWorker(owner, worker) {
    return owner === "engine" ? this.#engineWorker === worker : this.#renderWorker === worker;
  }

  #recordStaleWorkerEvent(owner) {
    this.#staleWorkerEvents[owner] += 1;
  }

  #ownerDiagnostics(owner) {
    let pendingRequests = 0;
    for (const key of this.#pending.keys()) {
      if (key.startsWith(`${owner}:`)) {
        pendingRequests += 1;
      }
    }
    return Object.freeze({
      nextRequestId: this.#nextRequestIds[owner],
      pendingRequests,
      staleResponses: this.#staleResponses[owner],
      staleWorkerEvents: this.#staleWorkerEvents[owner],
    });
  }

  #markFatalOwner(owner) {
    if (owner === "render" || this.#fatalOwner === null) {
      this.#fatalOwner = owner;
    }
  }

  #notifyError(error, owner) {
    this.#markFatalOwner(owner);
    this.#onError?.(error, owner);
  }

  #notifyRecoverableError(error, owner) {
    if (this.#onRecoverableError !== null) {
      this.#onRecoverableError(error, owner);
      return;
    }
    console.warn(`[Noon execution] recoverable ${owner} error`, error);
  }

  #rememberPlaying(result) {
    if (typeof result?.playing === "boolean") {
      this.#playing = result.playing;
    }
  }

  #requireStarted() {
    if (this.#engineWorker === null || this.#renderWorker === null) {
      throw new Error("ExecutionWorkerClient has not been started");
    }
  }
}

function engineEnvelope(type, payload = {}) {
  return {
    channel: ENGINE_CHANNEL,
    protocolVersion: ENGINE_PROTOCOL_VERSION,
    type,
    ...payload,
  };
}

function renderEnvelope(type, payload = {}) {
  return {
    channel: RENDER_CHANNEL,
    protocolVersion: RENDER_PROTOCOL_VERSION,
    type,
    ...payload,
  };
}

function validateWorkerEnvelope(message, channel) {
  const version = channel === ENGINE_CHANNEL ? ENGINE_PROTOCOL_VERSION : RENDER_PROTOCOL_VERSION;
  if (
    !message ||
    typeof message !== "object" ||
    message.channel !== channel ||
    message.protocolVersion !== version
  ) {
    throw new Error(`received an invalid ${channel} worker envelope`);
  }
}

function validateWorkerOwner(owner) {
  if (!WORKER_OWNERS.includes(owner)) {
    throw new Error(`unknown execution worker owner ${owner}`);
  }
}

function validateSceneJson(sceneJson) {
  if (typeof sceneJson !== "string" || sceneJson.trim() === "") {
    throw new TypeError("scene must be non-empty JSON text");
  }
}

function validateLoopDurationSeconds(loopDurationSeconds) {
  if (!Number.isFinite(loopDurationSeconds) || loopDurationSeconds <= 0) {
    throw new TypeError("loop duration must be positive and finite");
  }
  return loopDurationSeconds;
}

function validateOptionalLoopDurationSeconds(loopDurationSeconds) {
  if (loopDurationSeconds === null || loopDurationSeconds === undefined) {
    return null;
  }
  return validateLoopDurationSeconds(loopDurationSeconds);
}

function validateSeekTimeSeconds(timeSeconds, loopDurationSeconds) {
  if (!Number.isFinite(timeSeconds) || timeSeconds < 0) {
    throw new TypeError("playback seek time must be finite and non-negative");
  }
  if (timeSeconds > loopDurationSeconds) {
    throw new RangeError(
      `playback seek time ${timeSeconds} exceeds loop duration ${loopDurationSeconds}`,
    );
  }
  return timeSeconds;
}

function validateCallbacks(callbacks) {
  if (!callbacks || typeof callbacks !== "object") {
    throw new TypeError("callback configuration must be an object");
  }
  if (!Number.isSafeInteger(callbacks.session_id) || callbacks.session_id < 0) {
    throw new TypeError("callback configuration has an invalid session ID");
  }
  if (!Array.isArray(callbacks.slots) || callbacks.slots.length === 0) {
    throw new TypeError("callback configuration must contain slots");
  }
}

function cloneCallbacks(callbacks) {
  return {
    session_id: callbacks.session_id,
    slots: callbacks.slots.map(cloneCallbackSlot),
  };
}

function cloneCallbackSlot(slot) {
  const cloned = { id: slot.id, objects: [...slot.objects] };
  for (const field of ["active_after", "active_through"]) {
    if (Object.prototype.hasOwnProperty.call(slot, field)) {
      cloned[field] = slot[field];
    }
  }
  return cloned;
}

function checkedNextRequestId(current) {
  if (!Number.isSafeInteger(current) || current < 0 || current >= Number.MAX_SAFE_INTEGER) {
    throw new Error("execution worker request ID space exhausted");
  }
  return current + 1;
}

function checkedNextSession(current) {
  if (!Number.isSafeInteger(current) || current < 0 || current >= Number.MAX_SAFE_INTEGER) {
    throw new Error("execution worker session space exhausted");
  }
  return current + 1;
}
