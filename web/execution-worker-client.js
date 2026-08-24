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
    if (transportMode === EXECUTION_TRANSPORT_SHARED && selectExecutionTransportMode() !== EXECUTION_TRANSPORT_SHARED) {
      throw new Error("shared execution transport requires cross-origin isolation");
    }
    if (typeof this.#canvas.transferControlToOffscreen !== "function") {
      throw new Error("OffscreenCanvas transfer is unavailable in this browser");
    }

    this.#sceneJson = sceneJson;
    this.#loopDurationSeconds = loopDurationSeconds;
    this.#transportMode = transportMode;
    this.#session = checkedNextSession(this.#session);

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
        width: this.#canvas.width,
        height: this.#canvas.height,
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

  async replaceScene(sceneJson) {
    validateSceneJson(sceneJson);
    const result = await this.#requestEngine("replace_scene", { sceneJson });
    this.#sceneJson = result.sceneJson ?? sceneJson;
    return result;
  }

  async reconcileScene(sceneJson) {
    validateSceneJson(sceneJson);
    const result = await this.#requestEngine("reconcile_scene", { sceneJson });
    this.#sceneJson = result.sceneJson ?? sceneJson;
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

  async metrics() {
    return this.#requestRender("metrics", {});
  }

  resize(width, height, devicePixelRatio = 1) {
    this.#requireStarted();
    if (!Number.isFinite(width) || !Number.isFinite(height) || !Number.isFinite(devicePixelRatio)) {
      throw new TypeError("execution canvas dimensions must be finite");
    }
    const physicalWidth = Math.max(1, Math.round(width * devicePixelRatio));
    const physicalHeight = Math.max(1, Math.round(height * devicePixelRatio));
    this.#canvas.width = physicalWidth;
    this.#canvas.height = physicalHeight;
    this.#renderWorker.postMessage(
      renderEnvelope("resize", { width: physicalWidth, height: physicalHeight }),
    );
  }

  async restart() {
    this.#requireStarted();
    const sceneJson = this.#sceneJson;
    const loopDurationSeconds = this.#loopDurationSeconds;
    const transportMode = this.#transportMode;
    this.terminate();

    const previous = this.#canvas;
    const replacement = previous.cloneNode(false);
    replacement.width = previous.width;
    replacement.height = previous.height;
    replacement.className = previous.className;
    replacement.id = previous.id;
    previous.replaceWith(replacement);
    this.#canvas = replacement;
    return this.start(sceneJson, { loopDurationSeconds, transportMode });
  }

  terminate() {
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
  }

  async #requestEngine(type, payload) {
    await this.ready();
    return this.#request(this.#engineWorker, "engine", engineEnvelope, type, payload);
  }

  async #requestRender(type, payload) {
    await this.ready();
    return this.#request(this.#renderWorker, "render", renderEnvelope, type, payload);
  }

  #request(worker, owner, envelopeFactory, type, payload) {
    const requestId = this.#nextRequestId;
    this.#nextRequestId += 1;
    const result = new Promise((resolve, reject) => {
      this.#pending.set(`${owner}:${requestId}`, { resolve, reject });
    });
    worker.postMessage(envelopeFactory(type, { requestId, ...payload }));
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

function checkedNextSession(current) {
  if (!Number.isSafeInteger(current) || current < 0 || current >= Number.MAX_SAFE_INTEGER) {
    throw new Error("execution worker session space exhausted");
  }
  return current + 1;
}
