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
const SCENE_SPEC_VERSION = 1;
const AUTHORING_CANONICAL = "canonical";
const AUTHORING_COMPATIBILITY = "compatibility";
const DEFAULT_SHARED_SLOT_CAPACITY = 1024 * 1024;

export class RetainedExecutionWorkerClient {
  #canvas;
  #engineWorker = null;
  #renderWorker = null;
  #nextRequestId = 0;
  #pending = new Map();
  #session = 0;
  #authoring = null;
  #loopDurationSeconds = 4;
  #transportMode = null;
  #sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY;
  #ready = null;
  #playing = true;
  #onError;
  #onRecoverableError;
  #fatalOwner = null;
  #staleWorkerEvents = { engine: 0, render: 0 };

  constructor(canvas, { onError = null, onRecoverableError = null } = {}) {
    if (!(canvas instanceof HTMLCanvasElement)) {
      throw new TypeError("RetainedExecutionWorkerClient requires an HTMLCanvasElement");
    }
    if (onError !== null && typeof onError !== "function") {
      throw new TypeError("RetainedExecutionWorkerClient onError must be a function");
    }
    if (onRecoverableError !== null && typeof onRecoverableError !== "function") {
      throw new TypeError("RetainedExecutionWorkerClient onRecoverableError must be a function");
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
      authoring: this.#authoring?.kind ?? null,
      staleWorkerEvents: Object.freeze({ ...this.#staleWorkerEvents }),
    });
  }

  /// Compatibility entry point for the transitional split authoring payload.
  async start(sceneJson, retainedDocumentJson, options = {}) {
    validateLegacySceneJson(sceneJson);
    validateRetainedDocumentJson(retainedDocumentJson);
    return this.#startAuthoring(
      Object.freeze({
        kind: AUTHORING_COMPATIBILITY,
        sceneJson,
        retainedDocumentJson,
      }),
      options,
    );
  }

  /// Canonical retained startup. Browser execution after #367 should use this path.
  async startCanonical(sceneSpecJson, options = {}) {
    validateSceneSpecJson(sceneSpecJson);
    return this.#startAuthoring(
      Object.freeze({ kind: AUTHORING_CANONICAL, sceneSpecJson }),
      options,
    );
  }

  async #startAuthoring(
    authoring,
    {
      loopDurationSeconds = 4,
      transportMode = selectExecutionTransportMode(),
      sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY,
    } = {},
  ) {
    if (this.#engineWorker !== null || this.#renderWorker !== null) {
      throw new Error("RetainedExecutionWorkerClient is already started");
    }
    validateAuthoringPayload(authoring);
    validateLoopDurationSeconds(loopDurationSeconds);
    validateSharedSlotCapacity(sharedSlotCapacity);
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

    this.#authoring = authoring;
    this.#loopDurationSeconds = loopDurationSeconds;
    this.#transportMode = transportMode;
    this.#sharedSlotCapacity = sharedSlotCapacity;
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
        { type: "module", name: "noon-retained-engine" },
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
      this.#postEngineInit(this.#engineWorker, channel.port1);
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

  async restart({ failedOwner = this.#fatalOwner } = {}) {
    if (this.#authoring === null || this.#transportMode === null) {
      throw new Error("RetainedExecutionWorkerClient has not been started");
    }
    if (failedOwner !== null && failedOwner !== "engine" && failedOwner !== "render") {
      throw new TypeError(`unsupported failed mixed retained owner ${failedOwner}`);
    }
    if (failedOwner === "engine" && this.#renderWorker !== null) {
      return this.#restartEngine();
    }
    return this.#restartAll();
  }

  async #restartEngine() {
    const wasPlaying = this.#playing;
    const reconnectError = new Error("mixed retained engine worker restarting");
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
    this.#session = checkedNext(this.#session, "mixed retained execution session");
    this.#engineWorker = new Worker(
      new URL("./retained-execution-engine-worker.js", import.meta.url),
      { type: "module", name: "noon-retained-engine" },
    );
    const engineReady = this.#workerReady(this.#engineWorker, ENGINE_CHANNEL, "engine");
    const nextReady = Promise.all([engineReady, renderAttached]).then(([engine, render]) => ({
      engine,
      render,
      transportMode: this.#transportMode,
      session: this.#session,
    }));
    this.#ready = nextReady;
    this.#postEngineInit(this.#engineWorker, channel.port1);

    const ready = await nextReady;
    this.#playing = true;
    if (!wasPlaying) {
      await this.pause();
    }
    this.#fatalOwner = null;
    return ready;
  }

  async #restartAll() {
    const authoring = this.#authoring;
    const loopDurationSeconds = this.#loopDurationSeconds;
    const transportMode = this.#transportMode;
    const sharedSlotCapacity = this.#sharedSlotCapacity;
    const wasPlaying = this.#playing;
    if (this.#engineWorker !== null || this.#renderWorker !== null) {
      this.terminate();
      this.#canvas = replaceExecutionCanvas(this.#canvas);
    }
    const ready = await this.#startAuthoring(authoring, {
      loopDurationSeconds,
      transportMode,
      sharedSlotCapacity,
    });
    if (!wasPlaying) {
      await this.pause();
    }
    this.#fatalOwner = null;
    return ready;
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
    this.#fatalOwner = null;
  }

  #postEngineInit(worker, port) {
    if (this.#authoring === null) {
      throw new Error("retained execution authoring payload is unavailable");
    }
    worker.postMessage(
      engineEnvelope("init", {
        port,
        ...authoringWirePayload(this.#authoring),
        loopDurationSeconds: this.#loopDurationSeconds,
        transportMode: this.#transportMode,
        sharedSlotCapacity: this.#sharedSlotCapacity,
        session: this.#session,
      }),
      [port],
    );
  }

  async #requestEngine(type, payload, transfer = []) {
    await this.ready();
    return this.#request(this.#engineWorker, "engine", engineEnvelope, type, payload, transfer);
  }

  async #requestRender(type, payload, transfer = []) {
    await this.ready();
    return this.#request(this.#renderWorker, "render", renderEnvelope, type, payload, transfer);
  }

  #request(worker, owner, envelopeFactory, type, payload, transfer = []) {
    const requestId = this.#nextRequestId;
    this.#nextRequestId = checkedNext(this.#nextRequestId, "mixed retained worker request ID");
    const result = new Promise((resolve, reject) => {
      this.#pending.set(`${owner}:${requestId}`, { resolve, reject });
    });
    worker.postMessage(envelopeFactory(type, { requestId, ...payload }), transfer);
    return result;
  }

  #workerReady(worker, channel, owner) {
    return new Promise((resolve, reject) => {
      worker.addEventListener("message", (event) => {
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
          if (message.type === "recoverable_error") {
            const error = new Error(
              message.message || `${owner} mixed retained worker reported a recoverable error`,
            );
            error.diagnostic = message.diagnostic ?? null;
            this.#notifyRecoverableError(error, owner);
            return;
          }
          if (message.type === "error") {
            const error = new Error(message.message || `${owner} mixed retained worker failed`);
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
      });
      worker.addEventListener("error", (event) => {
        if (!this.#isCurrentWorker(owner, worker)) {
          this.#recordStaleWorkerEvent(owner);
          return;
        }
        const error = new Error(event.message || `${owner} mixed retained worker crashed`);
        reject(error);
        this.#rejectOwner(owner, error);
        this.#notifyError(error, owner);
      });
      worker.addEventListener("messageerror", () => {
        if (!this.#isCurrentWorker(owner, worker)) {
          this.#recordStaleWorkerEvent(owner);
          return;
        }
        const error = new Error(`${owner} mixed retained worker message could not be decoded`);
        reject(error);
        this.#rejectOwner(owner, error);
        this.#notifyError(error, owner);
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
    console.warn(`[Noon retained execution] recoverable ${owner} error`, error);
  }

  #rememberPlaying(result) {
    if (typeof result?.playing === "boolean") {
      this.#playing = result.playing;
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

function validateAuthoringPayload(authoring) {
  if (authoring?.kind === AUTHORING_CANONICAL) {
    validateSceneSpecJson(authoring.sceneSpecJson);
    return;
  }
  if (authoring?.kind === AUTHORING_COMPATIBILITY) {
    validateLegacySceneJson(authoring.sceneJson);
    validateRetainedDocumentJson(authoring.retainedDocumentJson);
    return;
  }
  throw new TypeError("unsupported retained execution authoring payload");
}

function authoringWirePayload(authoring) {
  if (authoring.kind === AUTHORING_CANONICAL) {
    return { sceneSpecJson: authoring.sceneSpecJson };
  }
  return {
    sceneJson: authoring.sceneJson,
    retainedDocumentJson: authoring.retainedDocumentJson,
  };
}

function validateSceneSpecJson(sceneSpecJson) {
  if (typeof sceneSpecJson !== "string" || sceneSpecJson.trim() === "") {
    throw new TypeError("canonical SceneSpec must be non-empty JSON text");
  }
  let document;
  try {
    document = JSON.parse(sceneSpecJson);
  } catch (error) {
    throw new TypeError(`canonical SceneSpec must be valid JSON: ${error}`);
  }
  if (!document || typeof document !== "object" || Array.isArray(document)) {
    throw new TypeError("canonical SceneSpec must decode to an object");
  }
  if (document.version !== SCENE_SPEC_VERSION) {
    throw new TypeError(`unsupported canonical SceneSpec version ${document.version}`);
  }
  if (!Array.isArray(document.objects) || !Array.isArray(document.tracks)) {
    throw new TypeError("canonical SceneSpec must contain objects and tracks arrays");
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

function validateSharedSlotCapacity(sharedSlotCapacity) {
  if (!Number.isSafeInteger(sharedSlotCapacity) || sharedSlotCapacity <= 0) {
    throw new TypeError("mixed retained shared slot capacity must be a positive safe integer");
  }
  return sharedSlotCapacity;
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
