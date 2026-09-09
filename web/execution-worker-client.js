import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  selectExecutionTransportMode,
} from "./execution-transport.js";
import { replaceExecutionCanvas } from "./execution-canvas.js";
import { MainThreadRenderWorker } from "./main-thread-render-worker.js";
import {
  RENDER_HOST_MAIN_THREAD,
  RENDER_HOST_WORKER,
  selectExecutionRenderHost,
} from "./render-host-selection.js";

const ENGINE_CHANNEL = "noon.engine";
const ENGINE_PROTOCOL_VERSION = 1;
const RENDER_CHANNEL = "noon.render";
const RENDER_PROTOCOL_VERSION = 1;
const WORKER_OWNERS = Object.freeze(["engine", "render"]);
const RENDER_MODE_RETAINED = "retained";
const EXECUTION_MODE_SEMANTIC = "semantic";
const SEMANTIC_PACING_REALTIME = "realtime";
const SEMANTIC_PACING_EXTERNAL_SAMPLES = "external_samples";
const DEFAULT_SHARED_SLOT_CAPACITY = 1024 * 1024;
const LIFECYCLE_CANCELLED_MESSAGE =
  "execution worker client was terminated during an asynchronous operation";

function workerCrashError(event, fallback) {
  const location =
    event.filename && `${event.filename}:${event.lineno ?? 0}:${event.colno ?? 0}`;
  const details = [event.message, event.error?.stack, location].filter(Boolean);
  return new Error(details.join("\n") || fallback);
}

export class ExecutionWorkerClient {
  #canvas;
  #engineWorker = null;
  #candidateEngineWorker = null;
  #candidateEngineReject = null;
  #renderWorker = null;
  #renderPrepared = null;
  #renderHost = null;
  #renderHostSelection = null;
  #preparedStartReservation = null;
  #nextRequestIds = { engine: 0, render: 0 };
  #pending = new Map();
  #session = 0;
  #loopDurationSeconds = 4;
  #transportMode = null;
  #sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY;
  #ready = null;
  #playing = true;
  #onError;
  #onRecoverableError;
  #semanticAuthoringClient = null;
  #semanticContextId = null;
  #semanticCallbackSessionId = null;
  #semanticPacing = SEMANTIC_PACING_REALTIME;
  #fatalOwner = null;
  #lifecycleGeneration = 0;
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

  get mode() {
    return EXECUTION_MODE_SEMANTIC;
  }

  get transportMode() {
    return this.#transportMode;
  }

  get renderHost() {
    return this.#renderHost;
  }

  get diagnostics() {
    return Object.freeze({
      session: this.#session,
      renderHost: this.#renderHost,
      engine: this.#ownerDiagnostics("engine"),
      render: this.#ownerDiagnostics("render"),
    });
  }

  async prepare(
    {
      transportMode = selectExecutionTransportMode(),
      sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY,
    } = {},
  ) {
    if (
      this.#engineWorker !== null ||
      this.#renderWorker !== null ||
      this.#preparedStartReservation !== null
    ) {
      throw new Error("ExecutionWorkerClient is already started or prepared");
    }
    validateTransportMode(transportMode);
    if (
      transportMode === EXECUTION_TRANSPORT_SHARED &&
      selectExecutionTransportMode() !== EXECUTION_TRANSPORT_SHARED
    ) {
      throw new Error("shared execution transport requires cross-origin isolation");
    }
    if (typeof this.#canvas.transferControlToOffscreen !== "function") {
      throw new Error("OffscreenCanvas transfer is unavailable in this browser");
    }

    const reservation = {};
    this.#preparedStartReservation = reservation;
    try {
      const generation = this.#lifecycleGeneration;
      const renderHost = this.#selectRenderHost();
      if (typeof renderHost !== "string") {
        await renderHost;
        this.#assertLifecycleCurrent(generation);
      }

      this.#transportMode = transportMode;
      this.#sharedSlotCapacity = validateSharedSlotCapacity(sharedSlotCapacity);
      const { width, height } = this.#prepareCanvasDimensions();
      const transferredCanvas = this.#canvas;

      let canvasTransferred = false;
      try {
        const offscreen = this.#canvas.transferControlToOffscreen();
        canvasTransferred = true;
        this.#renderWorker = this.#createRenderWorker();
        this.#attachCurrentWorkerEvents(this.#renderWorker, RENDER_CHANNEL, "render");
        this.#renderPrepared = this.#request(
          this.#renderWorker,
          "render",
          renderEnvelope,
          "prepare",
          {
            canvas: offscreen,
            transportMode,
            width,
            height,
          },
          [offscreen],
        );
        if (this.#preparedStartReservation === reservation) {
          this.#preparedStartReservation = null;
        }
        const render = await this.#renderPrepared;
        this.#fatalOwner = null;
        return { render, transportMode, renderHost: this.#renderHost };
      } catch (error) {
        if (generation === this.#lifecycleGeneration) {
          this.#rollbackFailedStart(
            error,
            canvasTransferred && this.#canvas === transferredCanvas,
          );
        }
        throw error;
      }
    } finally {
      if (this.#preparedStartReservation === reservation) {
        this.#preparedStartReservation = null;
      }
    }
  }

  async startSemanticExecution(contextId, authoringClient, options = {}) {
    validateSemanticContextId(contextId);
    validateSemanticAuthoringClient(authoringClient);
    const loopDurationSeconds = options.loopDurationSeconds ?? 4;
    const transportMode =
      options.transportMode ?? this.#transportMode ?? selectExecutionTransportMode();
    const sharedSlotCapacity =
      options.sharedSlotCapacity ??
      (this.#renderPrepared === null ? DEFAULT_SHARED_SLOT_CAPACITY : this.#sharedSlotCapacity);
    const callbackSessionId = validateOptionalCallbackSessionId(options.callbackSessionId);
    const continuationGeneration = validateOptionalContinuationGeneration(
      options.continuationGeneration,
    );
    const initiallyPaused = validateInitiallyPaused(options.initiallyPaused);
    const pacing = validateSemanticPacing(options.pacing);
    if (initiallyPaused && continuationGeneration !== null) {
      throw new Error("source-owned semantic continuations cannot start paused");
    }
    if (pacing === SEMANTIC_PACING_EXTERNAL_SAMPLES && continuationGeneration === null) {
      throw new Error("external sample pacing requires a source-owned semantic continuation");
    }
    if (this.#renderWorker === null) {
      await this.prepare({ transportMode, sharedSlotCapacity });
    }
    if (transportMode !== this.#transportMode) {
      throw new Error("prepared render transport mode does not match semantic execution startup");
    }
    if (sharedSlotCapacity !== this.#sharedSlotCapacity) {
      throw new Error("prepared shared slot capacity does not match semantic execution startup");
    }
    return this.#startPreparedSemantic(
      contextId,
      authoringClient,
      validateLoopDurationSeconds(loopDurationSeconds),
      callbackSessionId,
      continuationGeneration,
      initiallyPaused,
      pacing,
    );
  }

  async #startPreparedSemantic(
    contextId,
    authoringClient,
    loopDurationSeconds,
    callbackSessionId,
    continuationGeneration,
    initiallyPaused,
    pacing,
  ) {
    if (this.#engineWorker !== null || this.#preparedStartReservation !== null) {
      throw new Error("ExecutionWorkerClient is already started");
    }
    const generation = this.#lifecycleGeneration;
    const reservation = {};
    this.#preparedStartReservation = reservation;
    try {
      await this.#renderPrepared;
      this.#assertLifecycleCurrent(generation);
      this.#loopDurationSeconds = loopDurationSeconds;
      this.#session = checkedNextSession(this.#session);
      const control = new MessageChannel();
      const render = new MessageChannel();
      this.#engineWorker = control.port1;
      startEndpoint(this.#engineWorker);
      const engineReady = this.#workerReady(this.#engineWorker, ENGINE_CHANNEL, "engine");
      const renderReady = this.#request(
        this.#renderWorker,
        "render",
        renderEnvelope,
        "start_engine",
        {
          port: render.port2,
          transportMode: this.#transportMode,
          mode: RENDER_MODE_RETAINED,
        },
        [render.port2],
      );
      const attached = authoringClient.attachSemanticExecution(
        contextId,
        control.port2,
        render.port1,
        this.#semanticAttachmentOptions(
          loopDurationSeconds,
          this.#session,
          callbackSessionId,
          continuationGeneration,
          initiallyPaused,
          pacing,
        ),
      );
      this.#ready = Promise.all([engineReady, renderReady, attached]).then(
        ([engine, renderResult]) => ({
          engine,
          render: renderResult,
          transportMode: this.#transportMode,
          session: this.#session,
        }),
      );
      const ready = await this.#ready;
      this.#assertLifecycleCurrent(generation);
      this.#renderPrepared = null;
      this.#semanticAuthoringClient = authoringClient;
      this.#semanticContextId = contextId;
      this.#semanticCallbackSessionId = callbackSessionId;
      this.#semanticPacing = pacing;
      this.#playing = !initiallyPaused;
      this.#fatalOwner = null;
      return ready;
    } catch (error) {
      this.#assertLifecycleCurrent(generation);
      this.#renderPrepared = null;
      this.#rollbackFailedStart(error, true);
      throw error;
    } finally {
      if (this.#preparedStartReservation === reservation) {
        this.#preparedStartReservation = null;
      }
    }
  }

  #semanticAttachmentOptions(
    loopDurationSeconds,
    session,
    callbackSessionId = null,
    continuationGeneration = null,
    initiallyPaused = false,
    pacing = SEMANTIC_PACING_REALTIME,
  ) {
    const options = {
      transportMode: this.#transportMode,
      sharedSlotCapacity: this.#sharedSlotCapacity,
      loopDurationSeconds,
      session,
      initiallyPaused,
      pacing,
    };
    if (callbackSessionId !== null) options.callbackSessionId = callbackSessionId;
    if (continuationGeneration !== null) {
      options.continuationGeneration = continuationGeneration;
    }
    return options;
  }

  #prepareCanvasDimensions() {
    const devicePixelRatio = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.round(this.#canvas.clientWidth * devicePixelRatio));
    const height = Math.max(1, Math.round(this.#canvas.clientHeight * devicePixelRatio));
    this.#canvas.width = width;
    this.#canvas.height = height;
    return { width, height };
  }

  ready() {
    if (this.#ready === null) {
      throw new Error("ExecutionWorkerClient has not been started");
    }
    return this.#ready;
  }

  async switchToSemanticExecution(
    contextId,
    authoringClient,
    { loopDurationSeconds = null, callbackSessionId = null, continuationGeneration = null } = {},
  ) {
    return this.#transitionSemanticExecution(contextId, authoringClient, {
      loopDurationSeconds: validateOptionalLoopDurationSeconds(loopDurationSeconds),
      callbackSessionId: validateOptionalCallbackSessionId(callbackSessionId),
      continuationGeneration: validateOptionalContinuationGeneration(continuationGeneration),
      renderCommand: "rebuild_engine",
    });
  }

  async #transitionSemanticExecution(
    contextId,
    authoringClient,
    {
      loopDurationSeconds,
      callbackSessionId,
      continuationGeneration = null,
      renderCommand,
      replaceExistingEndpoint = false,
    },
  ) {
    this.#requireStarted();
    validateSemanticContextId(contextId);
    validateSemanticAuthoringClient(authoringClient);
    const generation = this.#lifecycleGeneration;
    const previousSession = this.#session;
    const previousSemanticAuthoringClient = this.#semanticAuthoringClient;
    const previousSemanticContextId = this.#semanticContextId;
    const wasPlaying = this.#playing;
    const duration = loopDurationSeconds ?? this.#loopDurationSeconds;
    const oldEngine = this.#engineWorker;
    const nextSession = checkedNextSession(this.#session);
    const control = new MessageChannel();
    const render = new MessageChannel();
    const candidate = control.port1;
    startEndpoint(candidate);
    this.#candidateEngineWorker = candidate;

    let candidateReady;
    try {
      const candidateReadyPromise = this.#candidateWorkerReady(candidate);
      const attached = authoringClient.attachSemanticExecution(
        contextId,
        control.port2,
        render.port1,
        { ...this.#semanticAttachmentOptions(
          duration,
          nextSession,
          callbackSessionId,
          continuationGeneration,
          continuationGeneration === null ? !wasPlaying : false,
        ), replaceExistingEndpoint },
      );
      [candidateReady] = await Promise.all([candidateReadyPromise, attached]);
      this.#assertLifecycleCurrent(generation);
    } catch (error) {
      if (this.#candidateEngineWorker === candidate) {
        this.#candidateEngineWorker = null;
        retireSemanticEndpoint(candidate);
      }
      render.port2.close?.();
      if (generation !== this.#lifecycleGeneration) {
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      throw error;
    }

    const reconnectError = new Error("execution engine transitioning to semantic");
    this.#candidateEngineWorker = null;
    this.#engineWorker = candidate;
    this.#attachCurrentWorkerEvents(candidate, ENGINE_CHANNEL, "engine");
    this.#session = nextSession;
    this.#rejectOwner("engine", reconnectError);

    try {
      const renderSwitched = this.#request(
        this.#renderWorker,
        "render",
        renderEnvelope,
        renderCommand,
        {
          port: render.port2,
          transportMode: this.#transportMode,
          mode: RENDER_MODE_RETAINED,
        },
        [render.port2],
      ).catch((error) => {
        this.#markFatalOwner("render");
        throw error;
      });
      this.#ready = renderSwitched.then((renderResult) => ({
        engine: candidateReady,
        render: renderResult,
        transportMode: this.#transportMode,
        session: this.#session,
      }));
      const ready = await this.#ready;
      this.#assertLifecycleCurrent(generation);
      this.#playing = continuationGeneration === null ? wasPlaying : true;
      this.#semanticAuthoringClient = authoringClient;
      this.#semanticContextId = contextId;
      this.#semanticCallbackSessionId = callbackSessionId;
      this.#loopDurationSeconds = duration;
      this.#fatalOwner = null;
      retireSemanticEndpoint(oldEngine);
      if (previousSemanticContextId !== contextId) {
        await releaseSemanticContext(
          previousSemanticAuthoringClient,
          previousSemanticContextId,
        );
      }
      return ready;
    } catch (error) {
      if (generation !== this.#lifecycleGeneration) {
        retireSemanticEndpoint(oldEngine);
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      this.#fatalOwner = "render";
      retireSemanticEndpoint(candidate);
      this.#engineWorker = oldEngine;
      this.#session = previousSession;
      throw error;
    }
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

  // Advance one canonical session barrier to an exact authored time. The
  // semantic endpoint owns forward progression and callback ordering; callers
  // receive only after the matching renderer publication has presented.
  async advanceTo(timeSeconds) {
    this.#requireStarted();
    const time = validateSeekTimeSeconds(timeSeconds, this.#loopDurationSeconds);
    const result = await this.#requestEngine("advance_to", { time });
    this.#rememberPlaying(result);
    return result;
  }

  async debugFrame() {
    this.#requireStarted();
    const result = await this.#requestEngine("debug_frame", {});
    return result.debugFrame;
  }

  async sampleToAuthoredTime(timeSeconds, { stopAtSourceCompletion = false } = {}) {
    this.#requireStarted();
    if (this.#semanticPacing !== SEMANTIC_PACING_EXTERNAL_SAMPLES) {
      throw new Error("external authored-time sampling requires external sample pacing");
    }
    const time = validateAuthoredSampleTime(timeSeconds);
    if (typeof stopAtSourceCompletion !== "boolean") {
      throw new TypeError("stopAtSourceCompletion must be a boolean");
    }
    const result = await this.#requestEngine("sample_to_authored_time", { time, stopAtSourceCompletion });
    this.#rememberPlaying(result);
    return result;
  }

  // Opt into one callback-publication renderer observation at this exact
  // authored time. The semantic and render workers produce and match the
  // publication metadata; this client retains no scene or renderer mirror.
  async advanceToWithRendererObservation(timeSeconds) {
    this.#requireStarted();
    const time = validateSeekTimeSeconds(timeSeconds, this.#loopDurationSeconds);
    const result = await this.#requestEngine("advance_to", {
      time,
      observeRenderer: true,
    });
    this.#rememberPlaying(result);
    return result;
  }

  async restartPlayback() {
    const result = await this.#requestEngine("restart_playback", {});
    this.#rememberPlaying(result);
    return result;
  }

  // Forward one normalized semantic native-state sample to the canonical session.
  async setNativeStateInput(source, value) {
    this.#requireStarted();
    return this.#requestEngine("native_state_input", { source, value });
  }

  // Forward one normalized semantic native-event source to the canonical session.
  async emitNativeEvent(source) {
    this.#requireStarted();
    return this.#requestEngine("native_event", { source });
  }

  async state() {
    return this.#requestEngine("state", {});
  }

  async metrics() {
    const [render, engine] = await Promise.all([
      this.#requestRender("metrics", {}),
      this.#requestEngine("metrics", {}),
    ]);
    return { ...render, engineMetrics: engine.metrics, renderHost: this.#renderHost };
  }

  resize(width, height, devicePixelRatio = 1) {
    this.#requireStarted();
    if (!Number.isFinite(width) || !Number.isFinite(height) || !Number.isFinite(devicePixelRatio)) {
      throw new TypeError("execution canvas dimensions must be finite");
    }
    const physicalWidth = Math.max(1, Math.round(width * devicePixelRatio));
    const physicalHeight = Math.max(1, Math.round(height * devicePixelRatio));
    this.#renderWorker.postMessage(
      renderEnvelope("resize", { width: physicalWidth, height: physicalHeight }),
    );
  }

  async restart({ failedOwner = this.#fatalOwner } = {}) {
    const hasAuthoring = this.#semanticContextId !== null && this.#semanticAuthoringClient !== null;
    if (!hasAuthoring || this.#transportMode === null) {
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
    return this.#transitionSemanticExecution(
      this.#semanticContextId,
      this.#semanticAuthoringClient,
      {
        loopDurationSeconds: this.#loopDurationSeconds,
        callbackSessionId: this.#semanticCallbackSessionId,
        renderCommand: "rebuild_engine",
        replaceExistingEndpoint: true,
      },
    );
  }

  async #restartAll() {
    const loopDurationSeconds = this.#loopDurationSeconds;
    const transportMode = this.#transportMode;
    const sharedSlotCapacity = this.#sharedSlotCapacity;
    const wasPlaying = this.#playing;
    const semanticAuthoringClient = this.#semanticAuthoringClient;
    const semanticContextId = this.#semanticContextId;
    const semanticCallbackSessionId = this.#semanticCallbackSessionId;

    if (this.#engineWorker !== null || this.#renderWorker !== null) {
      this.terminate({ preserveHostConfiguration: true });
    }

    const ready = await this.startSemanticExecution(semanticContextId, semanticAuthoringClient, {
      loopDurationSeconds,
      transportMode,
      sharedSlotCapacity,
      callbackSessionId: semanticCallbackSessionId,
      initiallyPaused: !wasPlaying,
    });
    this.#fatalOwner = null;
    return ready;
  }

  terminate({ preserveHostConfiguration = false } = {}) {
    // The render owner holds the transferred OffscreenCanvas whether or not an
    // engine has attached yet. Once that owner is terminated, replace the DOM
    // canvas exactly once so a later owner can transfer a fresh element.
    const restoreTransferredCanvas = this.#renderWorker !== null;
    this.#lifecycleGeneration += 1;
    const cancellation = new Error(LIFECYCLE_CANCELLED_MESSAGE);
    this.#candidateEngineReject?.(cancellation);
    this.#candidateEngineReject = null;
    closeEndpoint(this.#candidateEngineWorker);
    this.#candidateEngineWorker = null;
    retireSemanticEndpoint(this.#engineWorker);
    if (!preserveHostConfiguration) {
      void releaseSemanticContext(
        this.#semanticAuthoringClient,
        this.#semanticContextId,
      );
    }
    this.#renderWorker?.terminate();
    this.#engineWorker = null;
    this.#renderWorker = null;
    this.#renderPrepared = null;
    this.#preparedStartReservation = null;
    this.#ready = null;
    const error = new Error("execution worker client terminated");
    for (const pending of this.#pending.values()) {
      pending.reject(error);
    }
    this.#pending.clear();
    if (restoreTransferredCanvas) {
      this.#canvas = replaceExecutionCanvas(this.#canvas);
    }
    if (!preserveHostConfiguration) {
      this.#renderHost = null;
      this.#renderHostSelection = null;
      this.#semanticAuthoringClient = null;
      this.#semanticContextId = null;
      this.#semanticCallbackSessionId = null;
      this.#semanticPacing = SEMANTIC_PACING_REALTIME;
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
    if (worker === null) {
      throw new Error(`${owner} worker is unavailable`);
    }
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
      this.#attachCurrentWorkerEvents(worker, channel, owner, {
        resolveReady: resolve,
        rejectReady: reject,
      });
    });
  }

  #attachCurrentWorkerEvents(
    worker,
    channel,
    owner,
    { resolveReady = null, rejectReady = null } = {},
  ) {
    validateWorkerOwner(owner);
    const rejectInitial = (error) => rejectReady?.(error);
    const onMessage = (event) => {
      if (!this.#isCurrentWorker(owner, worker)) {
        this.#recordStaleWorkerEvent(owner);
        return;
      }
      const message = event.data;
      try {
        validateWorkerEnvelope(message, channel);
        if (message.type === "ready") {
          resolveReady?.(message);
          return;
        }
        if (message.type === "recoverable_error") {
          const error = new Error(
            message.message || `${owner} worker reported a recoverable error`,
          );
          error.diagnostic = message.diagnostic ?? null;
          this.#notifyRecoverableError(error, owner);
          return;
        }
        if (message.type === "error") {
          const error = new Error(message.message || `${owner} worker failed`);
          if (message.requestId === null || message.requestId === undefined) {
            rejectInitial(error);
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
        rejectInitial(error);
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
      const error = workerCrashError(event, `${owner} worker crashed`);
      rejectInitial(error);
      this.#rejectOwner(owner, error);
      this.#notifyError(error, owner);
    });
    worker.addEventListener("messageerror", () => {
      if (!this.#isCurrentWorker(owner, worker)) {
        this.#recordStaleWorkerEvent(owner);
        return;
      }
      const error = new Error(`${owner} worker message could not be decoded`);
      rejectInitial(error);
      this.#rejectOwner(owner, error);
      this.#notifyError(error, owner);
    });
  }

  #candidateWorkerReady(worker) {
    return new Promise((resolve, reject) => {
      let settled = false;
      const settle = (callback, value) => {
        if (settled) {
          return;
        }
        settled = true;
        if (this.#candidateEngineReject === cancel) {
          this.#candidateEngineReject = null;
        }
        callback(value);
      };
      const cancel = (error) => settle(reject, error);
      this.#candidateEngineReject = cancel;

      worker.addEventListener("message", (event) => {
        if (this.#candidateEngineWorker !== worker) {
          return;
        }
        try {
          const message = event.data;
          validateWorkerEnvelope(message, ENGINE_CHANNEL);
          if (message.type === "ready") {
            settle(resolve, message);
            return;
          }
          if (message.type === "error") {
            settle(reject, new Error(message.message || "candidate engine worker failed"));
            return;
          }
          throw new Error(
            `candidate engine emitted unexpected ${message.type ?? "message"} before ready`,
          );
        } catch (error) {
          settle(reject, error);
        }
      });
      worker.addEventListener("error", (event) => {
        if (this.#candidateEngineWorker === worker) {
          settle(reject, workerCrashError(event, "candidate engine worker crashed"));
        }
      });
      worker.addEventListener("messageerror", () => {
        if (this.#candidateEngineWorker === worker) {
          settle(reject, new Error("candidate engine worker message could not be decoded"));
        }
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
    retireSemanticEndpoint(this.#engineWorker);
    this.#renderWorker?.terminate();
    this.#engineWorker = null;
    this.#renderWorker = null;
    this.#renderPrepared = null;
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

  #selectRenderHost() {
    if (this.#renderHost !== null) return this.#renderHost;
    if (this.#renderHostSelection !== null) return this.#renderHostSelection;

    const selected = selectExecutionRenderHost();
    if (typeof selected === "string") {
      this.#renderHost = selected;
      return selected;
    }

    const generation = this.#lifecycleGeneration;
    const task = Promise.resolve(selected)
      .then((host) => {
        if (generation !== this.#lifecycleGeneration) {
          throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
        }
        this.#renderHost = host;
        return host;
      })
      .finally(() => {
        if (this.#renderHostSelection === task) {
          this.#renderHostSelection = null;
        }
      });
    this.#renderHostSelection = task;
    return task;
  }

  #createRenderWorker() {
    if (this.#renderHost === RENDER_HOST_MAIN_THREAD) {
      return new MainThreadRenderWorker();
    }
    if (this.#renderHost === RENDER_HOST_WORKER) {
      return new Worker(new URL("./execution-render-worker.js", import.meta.url), {
        type: "module",
        name: "noon-render",
      });
    }
    throw new Error("execution render host must be selected before renderer startup");
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

  #assertLifecycleCurrent(generation) {
    if (generation !== this.#lifecycleGeneration) {
      throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
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

function validateSemanticContextId(contextId) {
  if (typeof contextId !== "string" || contextId.trim() === "") {
    throw new TypeError("semantic execution context ID must be a non-empty string");
  }
  return contextId;
}

function validateSemanticAuthoringClient(authoringClient) {
  if (
    !authoringClient ||
    typeof authoringClient.attachSemanticExecution !== "function" ||
    typeof authoringClient.releaseSemanticExecution !== "function"
  ) {
    throw new TypeError("semantic execution requires a Python authoring client");
  }
}

async function releaseSemanticContext(authoringClient, contextId) {
  if (authoringClient === null || authoringClient === undefined || contextId === null) return;
  try {
    await authoringClient.releaseSemanticExecution(contextId);
  } catch (error) {
    console.warn(`[Noon execution] failed to release semantic context ${contextId}`, error);
  }
}

function startEndpoint(endpoint) {
  endpoint?.start?.();
}

function closeEndpoint(endpoint) {
  endpoint?.terminate?.();
  if (typeof endpoint?.terminate !== "function") {
    endpoint?.close?.();
  }
}

function retireSemanticEndpoint(endpoint) {
  if (endpoint === null || endpoint === undefined) return;
  try {
    endpoint.postMessage(engineEnvelope("stop"));
  } catch {
    closeEndpoint(endpoint);
    return;
  }
  queueMicrotask(() => closeEndpoint(endpoint));
}

function validateTransportMode(transportMode) {
  if (
    transportMode !== EXECUTION_TRANSPORT_SHARED &&
    transportMode !== EXECUTION_TRANSPORT_TRANSFERABLE
  ) {
    throw new TypeError(`unsupported execution transport mode ${transportMode}`);
  }
  return transportMode;
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

function validateOptionalCallbackSessionId(callbackSessionId) {
  if (callbackSessionId === null || callbackSessionId === undefined) {
    return null;
  }
  if (!Number.isSafeInteger(callbackSessionId) || callbackSessionId < 0) {
    throw new TypeError("semantic callback session must be a non-negative safe integer");
  }
  return callbackSessionId;
}

function validateOptionalContinuationGeneration(generation) {
  if (generation === null || generation === undefined) return null;
  if (!Number.isSafeInteger(generation) || generation <= 0) {
    throw new TypeError("semantic continuation generation must be a positive safe integer");
  }
  return generation;
}

function validateInitiallyPaused(initiallyPaused) {
  if (initiallyPaused === null || initiallyPaused === undefined) return false;
  if (typeof initiallyPaused !== "boolean") {
    throw new TypeError("initiallyPaused must be a boolean");
  }
  return initiallyPaused;
}

function validateSemanticPacing(pacing) {
  if (pacing === null || pacing === undefined) return SEMANTIC_PACING_REALTIME;
  if (pacing !== SEMANTIC_PACING_REALTIME && pacing !== SEMANTIC_PACING_EXTERNAL_SAMPLES) {
    throw new TypeError(`unsupported semantic execution pacing ${pacing}`);
  }
  return pacing;
}

function validateAuthoredSampleTime(timeSeconds) {
  if (!Number.isFinite(timeSeconds) || timeSeconds < 0) {
    throw new TypeError("external authored-time sample must be finite and non-negative");
  }
  return timeSeconds;
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

function validateSharedSlotCapacity(sharedSlotCapacity) {
  if (!Number.isSafeInteger(sharedSlotCapacity) || sharedSlotCapacity <= 0) {
    throw new TypeError("shared execution slot capacity must be a positive safe integer");
  }
  return sharedSlotCapacity;
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
