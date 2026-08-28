import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  selectExecutionTransportMode,
} from "./execution-transport.js";
import { replaceExecutionCanvas } from "./execution-canvas.js";

const ENGINE_CHANNEL = "noon.engine";
const ENGINE_PROTOCOL_VERSION = 1;
const RENDER_CHANNEL = "noon.render";
const RENDER_PROTOCOL_VERSION = 1;
const RETAINED_AUTHORING_CHANNEL = "noon.authoring.retained";

export class RetainedExecutionWorkerClient {
  #canvas;
  #engineWorker = null;
  #renderWorker = null;
  #nextRequestId = 0;
  #pending = new Map();
  #session = 0;
  #sceneJson = null;
  #retainedDocumentJson = null;
  #loopDurationSeconds = 4;
  #transportMode = null;
  #ready = null;
  #onError;

  constructor(canvas, { onError = null } = {}) {
    if (!(canvas instanceof HTMLCanvasElement)) {
      throw new TypeError("RetainedExecutionWorkerClient requires an HTMLCanvasElement");
    }
    if (onError !== null && typeof onError !== "function") {
      throw new TypeError("RetainedExecutionWorkerClient onError must be a function");
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
    retainedDocumentJson,
    {
      loopDurationSeconds = 4,
      transportMode = selectExecutionTransportMode(),
      sharedSlotCapacity = 1024 * 1024,
    } = {},
  ) {
    if (this.#engineWorker !== null || this.#renderWorker !== null) {
      throw new Error("RetainedExecutionWorkerClient is already started");
    }
    validateLegacySceneJson(sceneJson);
    validateRetainedDocumentJson(retainedDocumentJson);
    validateLoopDurationSeconds(loopDurationSeconds);
    if (
      transportMode !== EXECUTION_TRANSPORT_SHARED &&
      transportMode !== EXECUTION_TRANSPORT_TRANSFERABLE
    ) {
      throw new TypeError(`unsupported mixed retained execution transport mode ${transportMode}`);
    }
    if (
      transportMode === EXECUTION_TRANSPORT_SHARED &&
      selectExecutionTransportMode() !== EXECUTION_TRANSPORT_SHARED
    ) {
      throw new Error("shared mixed retained execution transport requires cross-origin isolation");
    }
    if (typeof this.#canvas.transferControlToOffscreen !== "function") {
      throw new Error("OffscreenCanvas transfer is unavailable in this browser");
    }

    this.#sceneJson = sceneJson;
    this.#retainedDocumentJson = retainedDocumentJson;
    this.#loopDurationSeconds = loopDurationSeconds;
    this.#transportMode = transportMode;
    this.#session = checkedNext(this.#session, "mixed retained execution session");

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
      this.#engineWorker = new Worker(
        new URL("./retained-execution-engine-worker.js", import.meta.url),
        { type: "module", name: "noon-mixed-retained-engine" },
      );
      this.#renderWorker = new Worker(
        new URL("./retained-execution-render-worker.js", import.meta.url),
        { type: "module", name: "noon-mixed-retained-render" },
      );

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
          retainedDocumentJson,
          loopDurationSeconds,
          transportMode,
          sharedSlotCapacity,
          session: this.#session,
        }),
        [channel.port1],
      );
      return await this.#ready;
    } catch (error) {
      this.#rollbackFailedStart(error, canvasTransferred);
      throw error;
    }
  }

  ready() {
    if (this.#ready === null) {
      throw new Error("RetainedExecutionWorkerClient has not been started");
    }
    return this.#ready;
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

  async setLoopDurationSeconds(loopDurationSeconds) {
    const duration = validateLoopDurationSeconds(loopDurationSeconds);
    const result = await this.#requestEngine("set_loop_duration", {
      loopDurationSeconds: duration,
    });
    this.#loopDurationSeconds = duration;
    return result;
  }

  async pause() {
    return this.#requestEngine("pause", {});
  }

  async resume() {
    return this.#requestEngine("resume", {});
  }

  async seek(timeSeconds) {
    const time = validateSeekTimeSeconds(timeSeconds, this.#loopDurationSeconds);
    return this.#requestEngine("seek", { time });
  }

  async restartPlayback() {
    return this.#requestEngine("restart_playback", {});
  }

  resize(width, height, devicePixelRatio = 1) {
    this.#requireStarted();
    if (!Number.isFinite(width) || !Number.isFinite(height) || !Number.isFinite(devicePixelRatio)) {
      throw new TypeError("mixed retained execution canvas dimensions must be finite");
    }
    const physicalWidth = Math.max(1, Math.round(width * devicePixelRatio));
    const physicalHeight = Math.max(1, Math.round(height * devicePixelRatio));
    this.#renderWorker.postMessage(
      renderEnvelope("resize", { width: physicalWidth, height: physicalHeight }),
    );
  }

  async restart() {
    if (
      this.#sceneJson === null ||
      this.#retainedDocumentJson === null ||
      this.#transportMode === null
    ) {
      throw new Error("RetainedExecutionWorkerClient has not been started");
    }
    const sceneJson = this.#sceneJson;
    const retainedDocumentJson = this.#retainedDocumentJson;
    const loopDurationSeconds = this.#loopDurationSeconds;
    const transportMode = this.#transportMode;
    if (this.#engineWorker !== null || this.#renderWorker !== null) {
      this.terminate();
      this.#canvas = replaceExecutionCanvas(this.#canvas);
    }
    return this.start(sceneJson, retainedDocumentJson, {
      loopDurationSeconds,
      transportMode,
    });
  }

  terminate() {
    this.#engineWorker?.terminate();
    this.#renderWorker?.terminate();
    this.#engineWorker = null;
    this.#renderWorker = null;
    this.#ready = null;
    const error = new Error("mixed retained execution worker client terminated");
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
    this.#nextRequestId = checkedNext(this.#nextRequestId, "mixed retained worker request ID");
    const result = new Promise((resolve, reject) => {
      this.#pending.set(`${owner}:${requestId}`, { resolve, reject });
    });
    worker.postMessage(envelopeFactory(type, { requestId, ...payload }));
    return result;
  }

  #workerReady(worker, channel, owner) {
    return new Promise((resolve, reject) => {
      worker.addEventListener("message", (event) => {
        const message = event.data;
        try {
          validateWorkerEnvelope(message, channel);
          if (message.type === "ready") {
            resolve(message);
            return;
          }
          if (message.type === "error") {
            const error = new Error(message.message || `${owner} mixed retained worker failed`);
            if (message.requestId === null || message.requestId === undefined) {
              reject(error);
              this.#onError?.(error, owner);
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
          this.#onError?.(error, owner);
        }
      });
      worker.addEventListener("error", (event) => {
        const error = new Error(event.message || `${owner} mixed retained worker crashed`);
        reject(error);
        this.#rejectOwner(owner, error);
        this.#onError?.(error, owner);
      });
      worker.addEventListener("messageerror", () => {
        const error = new Error(`${owner} mixed retained worker message could not be decoded`);
        reject(error);
        this.#rejectOwner(owner, error);
        this.#onError?.(error, owner);
      });
    });
  }

  #settle(owner, requestId, settle) {
    if (!Number.isSafeInteger(requestId) || requestId < 0) {
      throw new Error(`${owner} mixed retained worker returned an invalid request ID`);
    }
    const key = `${owner}:${requestId}`;
    const pending = this.#pending.get(key);
    if (!pending) {
      throw new Error(`${owner} mixed retained worker returned unknown request ID ${requestId}`);
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
    if (replaceCanvas) {
      this.#canvas = replaceExecutionCanvas(this.#canvas);
    }
  }

  #requireStarted() {
    if (this.#engineWorker === null || this.#renderWorker === null) {
      throw new Error("RetainedExecutionWorkerClient has not been started");
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
    throw new Error(`received an invalid mixed retained ${channel} worker envelope`);
  }
}

function validateLegacySceneJson(sceneJson) {
  if (typeof sceneJson !== "string" || sceneJson.trim() === "") {
    throw new TypeError("legacy scene must be non-empty JSON text");
  }
  let document;
  try {
    document = JSON.parse(sceneJson);
  } catch (error) {
    throw new TypeError(`legacy scene must be valid JSON: ${error}`);
  }
  if (!document || typeof document !== "object" || !Array.isArray(document.objects)) {
    throw new TypeError("legacy scene JSON must contain an objects array");
  }
}

function validateRetainedDocumentJson(retainedDocumentJson) {
  if (typeof retainedDocumentJson !== "string" || retainedDocumentJson.trim() === "") {
    throw new TypeError("retained document must be non-empty JSON text");
  }
  let document;
  try {
    document = JSON.parse(retainedDocumentJson);
  } catch (error) {
    throw new TypeError(`retained document must be valid JSON: ${error}`);
  }
  if (document?.channel !== RETAINED_AUTHORING_CHANNEL) {
    throw new TypeError(`retained document must use ${RETAINED_AUTHORING_CHANNEL}`);
  }
}

function validateLoopDurationSeconds(loopDurationSeconds) {
  if (!Number.isFinite(loopDurationSeconds) || loopDurationSeconds <= 0) {
    throw new TypeError("mixed retained loop duration must be positive and finite");
  }
  return loopDurationSeconds;
}

function validateSeekTimeSeconds(timeSeconds, loopDurationSeconds) {
  if (!Number.isFinite(timeSeconds) || timeSeconds < 0) {
    throw new TypeError("mixed retained playback seek time must be finite and non-negative");
  }
  if (timeSeconds > loopDurationSeconds) {
    throw new RangeError(
      `mixed retained playback seek time ${timeSeconds} exceeds loop duration ${loopDurationSeconds}`,
    );
  }
  return timeSeconds;
}

function checkedNext(current, label) {
  if (!Number.isSafeInteger(current) || current < 0 || current >= Number.MAX_SAFE_INTEGER) {
    throw new Error(`${label} space exhausted`);
  }
  return current + 1;
}
