import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  selectExecutionTransportMode,
} from "./execution-transport.js";

const ENGINE_CHANNEL = "noon.engine";
const ENGINE_PROTOCOL_VERSION = 1;
const RENDER_CHANNEL = "noon.render";
const RENDER_PROTOCOL_VERSION = 1;

export class ExecutionWorkerClient {
  #canvas;
  #engineWorker = null;
  #renderWorker = null;
  #nextRequestId = 0;
  #pending = new Map();
  #session = 0;
  #sceneJson = null;
  #loopDurationSeconds = 4;
  #transportMode = null;
  #ready = null;
  #onError;
  #hostAuthoringClient = null;
  #hostCallbacks = null;

  constructor(canvas, { onError = null } = {}) {
    if (!(canvas instanceof HTMLCanvasElement)) {
      throw new TypeError("ExecutionWorkerClient requires an HTMLCanvasElement");
    }
    if (onError !== null && typeof onError !== "function") {
      throw new TypeError("ExecutionWorkerClient onError must be a function");
    }
    this.#canvas = canvas;
    this.#onError = onError;
  }

  get canvas() {
    return this.#canvas;
  }

  get transportMode() {
    return this.#transportMode;
  }

  async start(
    sceneJson,
    {
      loopDurationSeconds = 4,
      transportMode = selectExecutionTransportMode(),
      sharedSlotCapacity = 1024 * 1024,
    } = {},
  ) {
    if (this.#engineWorker !== null || this.#renderWorker !== null) {
      throw new Error("ExecutionWorkerClient is already started");
    }
    validateSceneJson(sceneJson);
    if (!Number.isFinite(loopDurationSeconds) || loopDurationSeconds <= 0) {
      throw new TypeError("loop duration must be positive and finite");
    }
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
    this.#session = checkedNextSession(this.#session);

    // Size the bitmap before handing ownership to the render worker. Creating a
    // WebGL surface from the browser-default 300×150 bitmap and immediately
    // reconfiguring it after startup is observably unreliable on some Chromium/
    // Linux WebGL paths, especially at fractional device scale factors.
    const initialSurface = initializeCanvasBackingStore(this.#canvas);
    const channel = new MessageChannel();
    const offscreen = this.#canvas.transferControlToOffscreen();
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
        width: initialSurface.width,
        height: initialSurface.height,
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
    return this.#ready;
  }

  ready() {
    if (this.#ready === null) {
      throw new Error("ExecutionWorkerClient has not been started");
    }
    return this.#ready;
  }

  async replaceScene(sceneJson, { callbacks = null, authoringClient = null } = {}) {
    validateSceneJson(sceneJson);
    const result = await this.#requestEngine("replace_scene", { sceneJson });
    this.#sceneJson = result.sceneJson ?? sceneJson;
    await this.configureHostCallbacks(callbacks, authoringClient);
    return result;
  }

  async reconcileScene(sceneJson, { callbacks = null, authoringClient = null } = {}) {
    validateSceneJson(sceneJson);
    const result = await this.#requestEngine("reconcile_scene", { sceneJson });
    this.#sceneJson = result.sceneJson ?? sceneJson;
    await this.configureHostCallbacks(callbacks, authoringClient);
    return result;
  }

  async applyPatchBatch(patchBatchJson) {
    if (typeof patchBatchJson !== "string" || patchBatchJson.trim() === "") {
      throw new TypeError("patch batch must be non-empty JSON text");
    }
    const result = await this.#requestEngine("apply_patch", { patchBatchJson });
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

  async restart() {
    this.#requireStarted();
    const sceneJson = this.#sceneJson;
    const loopDurationSeconds = this.#loopDurationSeconds;
    const transportMode = this.#transportMode;
    const callbacks = this.#hostCallbacks;
    const authoringClient = this.#hostAuthoringClient;
    this.terminate({ preserveHostConfiguration: true });

    const previous = this.#canvas;
    const replacement = previous.cloneNode(false);
    replacement.width = previous.width;
    replacement.height = previous.height;
    replacement.className = previous.className;
    replacement.id = previous.id;
    previous.replaceWith(replacement);
    this.#canvas = replacement;
    const ready = await this.start(sceneJson, { loopDurationSeconds, transportMode });
    if (callbacks !== null && authoringClient !== null) {
      this.#hostAuthoringClient = null;
      await this.configureHostCallbacks(callbacks, authoringClient);
    }
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
    const requestId = this.#nextRequestId;
    this.#nextRequestId = checkedNextRequestId(this.#nextRequestId);
    const result = new Promise((resolve, reject) => {
      this.#pending.set(`${owner}:${requestId}`, { resolve, reject });
    });
    worker.postMessage(envelopeFactory(type, { requestId, ...payload }), transfer);
    return result;
  }

  #workerReady(worker, channel, owner) {
    return new Promise((resolve, reject) => {
      const onMessage = (event) => {
        const message = event.data;
        try {
          validateWorkerEnvelope(message, channel);
          if (message.type === "ready") {
            resolve(message);
            return;
          }
          if (message.type === "host_callback_error") {
            this.#notifyError(new Error(message.message || "host callback failed"), "host");
            return;
          }
          if (message.type === "error") {
            const error = new Error(message.message || `${owner} worker failed`);
            if (message.requestId === null || message.requestId === undefined) {
              reject(error);
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
          this.#notifyError(error, owner);
        }
      };
      worker.addEventListener("message", onMessage);
      worker.addEventListener("error", (event) => {
        const error = new Error(event.message || `${owner} worker crashed`);
        reject(error);
        this.#rejectOwner(owner, error);
        this.#notifyError(error, owner);
      });
      worker.addEventListener("messageerror", () => {
        const error = new Error(`${owner} worker message could not be decoded`);
        reject(error);
        this.#rejectOwner(owner, error);
        this.#notifyError(error, owner);
      });
    });
  }

  #settle(owner, requestId, settle) {
    if (!Number.isSafeInteger(requestId) || requestId < 0) {
      throw new Error(`${owner} worker returned an invalid request ID`);
    }
    const key = `${owner}:${requestId}`;
    const pending = this.#pending.get(key);
    if (!pending) {
      throw new Error(`${owner} worker returned unknown request ID ${requestId}`);
    }
    this.#pending.delete(key);
    settle(pending);
  }

  #rejectOwner(owner, error) {
    for (const [key, pending] of this.#pending.entries()) {
      if (key.startsWith(`${owner}:`)) {
        pending.reject(error);
        this.#pending.delete(key);
      }
    }
  }

  #notifyError(error, owner) {
    this.#onError?.(error, owner);
  }

  #requireStarted() {
    if (this.#engineWorker === null || this.#renderWorker === null) {
      throw new Error("ExecutionWorkerClient has not been started");
    }
  }
}

function initializeCanvasBackingStore(canvas) {
  const cssWidth = canvas.clientWidth;
  const cssHeight = canvas.clientHeight;
  if (cssWidth <= 0 || cssHeight <= 0) {
    return { width: canvas.width, height: canvas.height };
  }

  const reportedScale = globalThis.devicePixelRatio;
  const scale = Number.isFinite(reportedScale) && reportedScale > 0 ? reportedScale : 1;
  const width = Math.max(1, Math.round(cssWidth * scale));
  const height = Math.max(1, Math.round(cssHeight * scale));
  canvas.width = width;
  canvas.height = height;
  return { width, height };
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

function validateSceneJson(sceneJson) {
  if (typeof sceneJson !== "string" || sceneJson.trim() === "") {
    throw new TypeError("scene must be non-empty JSON text");
  }
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
    slots: callbacks.slots.map((slot) => ({ id: slot.id, objects: [...slot.objects] })),
  };
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
