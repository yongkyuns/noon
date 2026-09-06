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
const EXECUTION_MODE_LEGACY = "legacy";
const EXECUTION_MODE_RETAINED = "retained";
const EXECUTION_MODE_SEMANTIC = "semantic";
const SCENE_SPEC_VERSION = 1;
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
  #preparedStartReservation = null;
  #nextRequestIds = { engine: 0, render: 0 };
  #pending = new Map();
  #session = 0;
  #mode = EXECUTION_MODE_LEGACY;
  #sceneJson = null;
  #sceneSpecJson = null;
  #loopDurationSeconds = 4;
  #transportMode = null;
  #sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY;
  #ready = null;
  #playing = true;
  #onError;
  #onRecoverableError;
  #hostAuthoringClient = null;
  #hostCallbacks = null;
  #semanticAuthoringClient = null;
  #semanticContextId = null;
  #semanticCallbackSessionId = null;
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
    return this.#mode;
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

  async prepare(
    {
      transportMode = selectExecutionTransportMode(),
      sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY,
    } = {},
  ) {
    if (this.#engineWorker !== null || this.#renderWorker !== null) {
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

    this.#transportMode = transportMode;
    this.#sharedSlotCapacity = validateSharedSlotCapacity(sharedSlotCapacity);
    const { width, height } = this.#prepareCanvasDimensions();
    const transferredCanvas = this.#canvas;
    const generation = this.#lifecycleGeneration;

    let canvasTransferred = false;
    try {
      const offscreen = this.#canvas.transferControlToOffscreen();
      canvasTransferred = true;
      this.#renderWorker = new Worker(new URL("./execution-render-worker.js", import.meta.url), {
        type: "module",
        name: "noon-render",
      });
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
      const render = await this.#renderPrepared;
      this.#fatalOwner = null;
      return { render, transportMode };
    } catch (error) {
      if (generation === this.#lifecycleGeneration) {
        this.#rollbackFailedStart(
          error,
          canvasTransferred && this.#canvas === transferredCanvas,
        );
      }
      throw error;
    }
  }

  async start(sceneJson, options = {}) {
    const loopDurationSeconds = options.loopDurationSeconds ?? 4;
    const transportMode =
      options.transportMode ?? this.#transportMode ?? selectExecutionTransportMode();
    const sharedSlotCapacity =
      options.sharedSlotCapacity ??
      (this.#renderPrepared === null ? DEFAULT_SHARED_SLOT_CAPACITY : this.#sharedSlotCapacity);
    validateSceneJson(sceneJson);
    sceneJson = projectLegacyReactiveSceneJson(sceneJson);
    return this.#startMode(EXECUTION_MODE_LEGACY, sceneJson, {
      loopDurationSeconds,
      transportMode,
      sharedSlotCapacity,
    });
  }

  async startRetainedCanonical(sceneSpecJson, options = {}) {
    const loopDurationSeconds = options.loopDurationSeconds ?? 4;
    const transportMode =
      options.transportMode ?? this.#transportMode ?? selectExecutionTransportMode();
    const sharedSlotCapacity =
      options.sharedSlotCapacity ??
      (this.#renderPrepared === null ? DEFAULT_SHARED_SLOT_CAPACITY : this.#sharedSlotCapacity);
    validateSceneSpecJson(sceneSpecJson);
    return this.#startMode(
      EXECUTION_MODE_RETAINED,
      null,
      { loopDurationSeconds, transportMode, sharedSlotCapacity },
      sceneSpecJson,
    );
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
    if (initiallyPaused && continuationGeneration !== null) {
      throw new Error("source-owned semantic continuations cannot start paused");
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
    );
  }

  async #startPreparedSemantic(
    contextId,
    authoringClient,
    loopDurationSeconds,
    callbackSessionId,
    continuationGeneration,
    initiallyPaused,
  ) {
    if (this.#engineWorker !== null || this.#preparedStartReservation !== null) {
      throw new Error("ExecutionWorkerClient is already started");
    }
    const reservation = {};
    this.#preparedStartReservation = reservation;
    try {
      await this.#renderPrepared;
      this.#configureStart(
        EXECUTION_MODE_SEMANTIC,
        null,
        loopDurationSeconds,
        this.#transportMode,
        this.#sharedSlotCapacity,
      );
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
          mode: EXECUTION_MODE_RETAINED,
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
      this.#renderPrepared = null;
      this.#semanticAuthoringClient = authoringClient;
      this.#semanticContextId = contextId;
      this.#semanticCallbackSessionId = callbackSessionId;
      this.#hostAuthoringClient = null;
      this.#hostCallbacks = null;
      this.#playing = !initiallyPaused;
      this.#fatalOwner = null;
      return ready;
    } catch (error) {
      this.#renderPrepared = null;
      this.#rollbackFailedStart(error, true);
      throw error;
    } finally {
      if (this.#preparedStartReservation === reservation) {
        this.#preparedStartReservation = null;
      }
    }
  }

  async #startMode(
    mode,
    sceneJson,
    {
      loopDurationSeconds,
      transportMode,
      sharedSlotCapacity,
    },
    sceneSpecJson = null,
  ) {
    if (this.#engineWorker !== null || this.#preparedStartReservation !== null) {
      throw new Error("ExecutionWorkerClient is already started");
    }
    const preparedRender = this.#renderWorker !== null;
    if (preparedRender && this.#renderPrepared === null) {
      throw new Error("ExecutionWorkerClient render owner is not in a prepared state");
    }
    validateExecutionMode(mode);
    if (mode === EXECUTION_MODE_LEGACY) {
      validateSceneJson(sceneJson);
      if (sceneSpecJson !== null) {
        throw new Error("legacy execution must not include canonical retained authoring");
      }
    } else if (mode === EXECUTION_MODE_RETAINED) {
      validateSceneSpecJson(sceneSpecJson);
      if (sceneJson !== null) {
        throw new Error("retained execution accepts only canonical SceneSpec authoring");
      }
    } else {
      throw new Error("semantic execution starts through startSemanticExecution");
    }
    validateLoopDurationSeconds(loopDurationSeconds);
    validateTransportMode(transportMode);
    const slotCapacity = validateSharedSlotCapacity(sharedSlotCapacity);
    if (
      transportMode === EXECUTION_TRANSPORT_SHARED &&
      selectExecutionTransportMode() !== EXECUTION_TRANSPORT_SHARED
    ) {
      throw new Error("shared execution transport requires cross-origin isolation");
    }

    if (preparedRender) {
      if (transportMode !== this.#transportMode) {
        throw new Error("prepared render transport mode does not match execution startup");
      }
      if (slotCapacity !== this.#sharedSlotCapacity) {
        throw new Error("prepared shared slot capacity does not match execution startup");
      }
      const reservation = {};
      this.#preparedStartReservation = reservation;
      try {
        await this.#renderPrepared;
        return await this.#startPreparedMode(
          mode,
          sceneJson,
          loopDurationSeconds,
          sceneSpecJson,
        );
      } finally {
        if (this.#preparedStartReservation === reservation) {
          this.#preparedStartReservation = null;
        }
      }
    }

    if (typeof this.#canvas.transferControlToOffscreen !== "function") {
      throw new Error("OffscreenCanvas transfer is unavailable in this browser");
    }

    this.#configureStart(
      mode,
      sceneJson,
      loopDurationSeconds,
      transportMode,
      slotCapacity,
      sceneSpecJson,
    );
    const { width: initialWidth, height: initialHeight } = this.#prepareCanvasDimensions();

    let canvasTransferred = false;
    try {
      const channel = new MessageChannel();
      const offscreen = this.#canvas.transferControlToOffscreen();
      canvasTransferred = true;
      this.#engineWorker = this.#createEngineWorker(mode);
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
          mode,
          width: initialWidth,
          height: initialHeight,
        }),
        [offscreen, channel.port2],
      );
      this.#postEngineInit(
        this.#engineWorker,
        channel.port1,
        mode,
        sceneJson,
        loopDurationSeconds,
        this.#session,
        sceneSpecJson,
      );
      const ready = await this.#ready;
      this.#playing = true;
      this.#fatalOwner = null;
      if (mode === EXECUTION_MODE_RETAINED) {
        this.#hostAuthoringClient = null;
        this.#hostCallbacks = null;
      }
      return ready;
    } catch (error) {
      this.#rollbackFailedStart(error, canvasTransferred);
      throw error;
    }
  }

  async #startPreparedMode(
    mode,
    sceneJson,
    loopDurationSeconds,
    sceneSpecJson = null,
  ) {
    this.#configureStart(
      mode,
      sceneJson,
      loopDurationSeconds,
      this.#transportMode,
      this.#sharedSlotCapacity,
      sceneSpecJson,
    );
    try {
      const channel = new MessageChannel();
      this.#engineWorker = this.#createEngineWorker(mode);
      const engineReady = this.#workerReady(this.#engineWorker, ENGINE_CHANNEL, "engine");
      const renderReady = this.#request(
        this.#renderWorker,
        "render",
        renderEnvelope,
        "start_engine",
        {
          port: channel.port2,
          transportMode: this.#transportMode,
          mode,
        },
        [channel.port2],
      );
      this.#ready = Promise.all([engineReady, renderReady]).then(([engine, render]) => ({
        engine,
        render,
        transportMode: this.#transportMode,
        session: this.#session,
      }));
      this.#postEngineInit(
        this.#engineWorker,
        channel.port1,
        mode,
        sceneJson,
        loopDurationSeconds,
        this.#session,
        sceneSpecJson,
      );
      const ready = await this.#ready;
      this.#renderPrepared = null;
      this.#playing = true;
      this.#fatalOwner = null;
      if (mode === EXECUTION_MODE_RETAINED) {
        this.#hostAuthoringClient = null;
        this.#hostCallbacks = null;
      }
      return ready;
    } catch (error) {
      this.#renderPrepared = null;
      this.#rollbackFailedStart(error, true);
      throw error;
    }
  }

  #configureStart(
    mode,
    sceneJson,
    loopDurationSeconds,
    transportMode,
    sharedSlotCapacity,
    sceneSpecJson = null,
  ) {
    this.#mode = mode;
    this.#sceneJson = sceneJson;
    this.#sceneSpecJson = mode === EXECUTION_MODE_RETAINED ? sceneSpecJson : null;
    this.#loopDurationSeconds = loopDurationSeconds;
    this.#transportMode = transportMode;
    this.#sharedSlotCapacity = sharedSlotCapacity;
    this.#session = checkedNextSession(this.#session);
  }

  #semanticAttachmentOptions(
    loopDurationSeconds,
    session,
    callbackSessionId = null,
    continuationGeneration = null,
    initiallyPaused = false,
  ) {
    const options = {
      transportMode: this.#transportMode,
      sharedSlotCapacity: this.#sharedSlotCapacity,
      loopDurationSeconds,
      session,
      initiallyPaused,
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

  async replaceScene(
    sceneJson,
    { callbacks = null, authoringClient = null, loopDurationSeconds = null } = {},
  ) {
    this.#requireLegacyMode("replace scenes");
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
    this.#requireLegacyMode("reconcile scenes");
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

  async switchToRetainedCanonical(sceneSpecJson, { loopDurationSeconds = null } = {}) {
    validateSceneSpecJson(sceneSpecJson);
    return this.#transitionEngine(EXECUTION_MODE_RETAINED, null, {
      loopDurationSeconds: validateOptionalLoopDurationSeconds(loopDurationSeconds),
      callbacks: null,
      authoringClient: null,
      renderCommand: "switch_engine",
      sceneSpecJson,
    });
  }

  async rebuildRetainedCanonical(sceneSpecJson, { loopDurationSeconds = null } = {}) {
    validateSceneSpecJson(sceneSpecJson);
    return this.#transitionEngine(EXECUTION_MODE_RETAINED, null, {
      loopDurationSeconds: validateOptionalLoopDurationSeconds(loopDurationSeconds),
      callbacks: null,
      authoringClient: null,
      renderCommand: "rebuild_engine",
      sceneSpecJson,
    });
  }

  async switchToSemanticExecution(
    contextId,
    authoringClient,
    { loopDurationSeconds = null, callbackSessionId = null } = {},
  ) {
    return this.#transitionSemanticExecution(contextId, authoringClient, {
      loopDurationSeconds: validateOptionalLoopDurationSeconds(loopDurationSeconds),
      callbackSessionId: validateOptionalCallbackSessionId(callbackSessionId),
      renderCommand:
        this.#mode === EXECUTION_MODE_LEGACY ? "switch_engine" : "rebuild_engine",
    });
  }

  async #transitionSemanticExecution(
    contextId,
    authoringClient,
    { loopDurationSeconds, callbackSessionId, renderCommand },
  ) {
    this.#requireStarted();
    validateSemanticContextId(contextId);
    validateSemanticAuthoringClient(authoringClient);
    const generation = this.#lifecycleGeneration;
    const previousMode = this.#mode;
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
        this.#semanticAttachmentOptions(
          duration,
          nextSession,
          callbackSessionId,
          null,
          !wasPlaying,
        ),
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
    if (previousMode !== EXECUTION_MODE_SEMANTIC) {
      closeEndpoint(oldEngine);
    }
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
          mode: EXECUTION_MODE_RETAINED,
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
      this.#playing = wasPlaying;
      this.#mode = EXECUTION_MODE_SEMANTIC;
      this.#sceneJson = null;
      this.#sceneSpecJson = null;
      this.#semanticAuthoringClient = authoringClient;
      this.#semanticContextId = contextId;
      this.#semanticCallbackSessionId = callbackSessionId;
      this.#loopDurationSeconds = duration;
      this.#hostAuthoringClient = null;
      this.#hostCallbacks = null;
      this.#fatalOwner = null;
      if (previousMode === EXECUTION_MODE_SEMANTIC) {
        retireSemanticEndpoint(oldEngine);
        if (previousSemanticContextId !== contextId) {
          await releaseSemanticContext(
            previousSemanticAuthoringClient,
            previousSemanticContextId,
          );
        }
      }
      return ready;
    } catch (error) {
      if (generation !== this.#lifecycleGeneration) {
        if (previousMode === EXECUTION_MODE_SEMANTIC) {
          retireSemanticEndpoint(oldEngine);
        }
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      this.#fatalOwner = "render";
      this.#mode = previousMode;
      retireSemanticEndpoint(candidate);
      if (previousMode === EXECUTION_MODE_SEMANTIC) {
        this.#engineWorker = oldEngine;
        this.#session = previousSession;
      }
      throw error;
    }
  }

  async switchToLegacy(
    sceneJson,
    {
      callbacks = null,
      authoringClient = null,
      loopDurationSeconds = null,
    } = {},
  ) {
    validateSceneJson(sceneJson);
    sceneJson = projectLegacyReactiveSceneJson(sceneJson);
    if (callbacks !== null && callbacks !== undefined) {
      validateCallbacks(callbacks);
      validateAuthoringClient(authoringClient);
    }
    return this.#transitionEngine(EXECUTION_MODE_LEGACY, sceneJson, {
      loopDurationSeconds: validateOptionalLoopDurationSeconds(loopDurationSeconds),
      callbacks,
      authoringClient,
      renderCommand: "switch_engine",
    });
  }

  async #transitionEngine(
    nextMode,
    sceneJson,
    { loopDurationSeconds, callbacks, authoringClient, renderCommand, sceneSpecJson = null },
  ) {
    this.#requireStarted();
    validateExecutionMode(nextMode);
    if (nextMode === EXECUTION_MODE_RETAINED) {
      validateSceneSpecJson(sceneSpecJson);
      if (sceneJson !== null) {
        throw new Error("retained execution accepts only canonical SceneSpec authoring");
      }
    } else {
      validateSceneJson(sceneJson);
      if (sceneSpecJson !== null) {
        throw new Error("legacy execution must not include canonical retained authoring");
      }
    }
    if (renderCommand === "switch_engine" && nextMode === this.#mode) {
      throw new Error(`execution worker client is already in ${nextMode} mode`);
    }
    if (
      renderCommand === "rebuild_engine" &&
      nextMode !== this.#mode &&
      !(this.#mode === EXECUTION_MODE_SEMANTIC &&
        (nextMode === EXECUTION_MODE_LEGACY || nextMode === EXECUTION_MODE_RETAINED))
    ) {
      throw new Error(
        `execution renderer rebuild mode ${nextMode} does not match active mode ${this.#mode}`,
      );
    }
    if (renderCommand !== "switch_engine" && renderCommand !== "rebuild_engine") {
      throw new Error(`unsupported execution renderer transition ${renderCommand}`);
    }

    const generation = this.#lifecycleGeneration;
    const previousMode = this.#mode;
    const previousSession = this.#session;
    const previousSemanticAuthoringClient = this.#semanticAuthoringClient;
    const previousSemanticContextId = this.#semanticContextId;
    const wasPlaying = this.#playing;
    const duration = loopDurationSeconds ?? this.#loopDurationSeconds;
    const oldEngine = this.#engineWorker;
    const nextSession = checkedNextSession(this.#session);
    const channel = new MessageChannel();
    const candidate = this.#createEngineWorker(nextMode);
    this.#candidateEngineWorker = candidate;

    let candidateReady;
    try {
      const candidateReadyPromise = this.#candidateWorkerReady(candidate);
      this.#postEngineInit(
        candidate,
        channel.port1,
        nextMode,
        sceneJson,
        duration,
        nextSession,
        sceneSpecJson,
      );
      candidateReady = await candidateReadyPromise;
      this.#assertLifecycleCurrent(generation);
    } catch (error) {
      if (this.#candidateEngineWorker === candidate) {
        this.#candidateEngineWorker = null;
        closeEndpoint(candidate);
      }
      channel.port2.close?.();
      if (generation !== this.#lifecycleGeneration) {
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      // Candidate initialization failed before the render owner was touched.
      // Keep the existing engine, renderer, mode, and recovery state authoritative.
      throw error;
    }

    // Candidate readiness is emitted only after its immutable resources/transport
    // setup and first complete snapshot have been queued on the MessageChannel.
    // Commit ownership only now, so engine/WASM bootstrap latency cannot blank the
    // live surface and preflight failure leaves the old session untouched.
    const reconnectError = new Error(`execution engine transitioning to ${nextMode}`);
    this.#candidateEngineWorker = null;
    this.#engineWorker = candidate;
    this.#attachCurrentWorkerEvents(candidate, ENGINE_CHANNEL, "engine");
    this.#session = nextSession;
    if (previousMode !== EXECUTION_MODE_SEMANTIC) {
      closeEndpoint(oldEngine);
    }
    this.#rejectOwner("engine", reconnectError);

    try {
      const renderSwitched = this.#request(
        this.#renderWorker,
        "render",
        renderEnvelope,
        renderCommand,
        {
          port: channel.port2,
          transportMode: this.#transportMode,
          mode: nextMode,
        },
        [channel.port2],
      ).catch((error) => {
        this.#markFatalOwner("render");
        throw error;
      });
      const nextReady = renderSwitched.then((render) => ({
        engine: candidateReady,
        render,
        transportMode: this.#transportMode,
        session: this.#session,
      }));
      this.#ready = nextReady;

      const ready = await nextReady;
      this.#assertLifecycleCurrent(generation);
      this.#playing = true;
      if (!wasPlaying) {
        const paused = await this.#requestEngine("pause", {});
        this.#rememberPlaying(paused);
        this.#assertLifecycleCurrent(generation);
      }

      if (
        nextMode === EXECUTION_MODE_LEGACY &&
        callbacks !== null &&
        callbacks !== undefined
      ) {
        this.#hostAuthoringClient = null;
        await this.#configureHostCallbacks(callbacks, authoringClient);
        this.#assertLifecycleCurrent(generation);
      } else {
        this.#hostAuthoringClient = null;
        this.#hostCallbacks = null;
      }

      this.#mode = nextMode;
      this.#sceneJson = sceneJson;
      this.#sceneSpecJson = nextMode === EXECUTION_MODE_RETAINED ? sceneSpecJson : null;
      this.#semanticAuthoringClient = null;
      this.#semanticContextId = null;
      this.#semanticCallbackSessionId = null;
      this.#loopDurationSeconds = duration;
      this.#fatalOwner = null;
      if (previousMode === EXECUTION_MODE_SEMANTIC) {
        retireSemanticEndpoint(oldEngine);
        await releaseSemanticContext(
          previousSemanticAuthoringClient,
          previousSemanticContextId,
        );
      }
      return ready;
    } catch (error) {
      if (generation !== this.#lifecycleGeneration) {
        if (previousMode === EXECUTION_MODE_SEMANTIC) {
          retireSemanticEndpoint(oldEngine);
        }
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      // Once the render transition begins, a failure can leave the live render
      // worker between renderer generations. Keep the conservative full-surface
      // recovery policy for this post-preflight failure boundary.
      this.#fatalOwner = "render";
      this.#mode = previousMode;
      closeEndpoint(candidate);
      if (previousMode === EXECUTION_MODE_SEMANTIC) {
        this.#engineWorker = oldEngine;
        this.#session = previousSession;
      }
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
    this.#requireSemanticMode("forward authored-time advancement");
    const time = validateSeekTimeSeconds(timeSeconds, this.#loopDurationSeconds);
    const result = await this.#requestEngine("advance_to", { time });
    this.#rememberPlaying(result);
    return result;
  }

  // Opt into one callback-publication renderer observation at this exact
  // authored time. The semantic and render workers produce and match the
  // publication metadata; this client retains no scene or renderer mirror.
  async advanceToWithRendererObservation(timeSeconds) {
    this.#requireSemanticMode("callback renderer observation");
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
    this.#requireSemanticMode("native state input");
    return this.#requestEngine("native_state_input", { source, value });
  }

  // Forward one normalized semantic native-event source to the canonical session.
  async emitNativeEvent(source) {
    this.#requireSemanticMode("native event input");
    return this.#requestEngine("native_event", { source });
  }

  async applyPatchBatch(patchBatchJson) {
    this.#requireLegacyMode("apply patch batches");
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
    this.#requireLegacyMode("configure host callbacks");
    await this.ready();
    return this.#configureHostCallbacks(callbacks, authoringClient);
  }

  async #configureHostCallbacks(callbacks, authoringClient) {
    if (callbacks === null || callbacks === undefined) {
      this.#hostCallbacks = null;
      await this.#requestEngine("configure_callbacks", { callbacks: null });
      return;
    }
    validateCallbacks(callbacks);
    validateAuthoringClient(authoringClient);
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
    this.#renderWorker.postMessage(
      renderEnvelope("resize", { width: physicalWidth, height: physicalHeight }),
    );
  }

  async restart({ failedOwner = this.#fatalOwner } = {}) {
    const hasAuthoring =
      this.#mode === EXECUTION_MODE_LEGACY
        ? this.#sceneJson !== null
        : this.#mode === EXECUTION_MODE_RETAINED
          ? this.#sceneSpecJson !== null
          : this.#semanticContextId !== null && this.#semanticAuthoringClient !== null;
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
    if (this.#mode === EXECUTION_MODE_SEMANTIC) {
      return this.#transitionSemanticExecution(
        this.#semanticContextId,
        this.#semanticAuthoringClient,
        {
          loopDurationSeconds: this.#loopDurationSeconds,
          callbackSessionId: this.#semanticCallbackSessionId,
          renderCommand: "rebuild_engine",
        },
      );
    }
    const generation = this.#lifecycleGeneration;
    const mode = this.#mode;
    const wasPlaying = this.#playing;
    const callbacks = mode === EXECUTION_MODE_LEGACY ? this.#hostCallbacks : null;
    const authoringClient =
      mode === EXECUTION_MODE_LEGACY ? this.#hostAuthoringClient : null;
    const reconnectError = new Error("execution engine worker restarting");
    closeEndpoint(this.#engineWorker);
    this.#engineWorker = null;
    this.#rejectOwner("engine", reconnectError);

    try {
      const channel = new MessageChannel();
      const renderAttached = this.#request(
        this.#renderWorker,
        "render",
        renderEnvelope,
        "attach_engine",
        {
          port: channel.port2,
          transportMode: this.#transportMode,
          mode,
        },
        [channel.port2],
      ).catch((error) => {
        this.#markFatalOwner("render");
        throw error;
      });
      this.#session = checkedNextSession(this.#session);
      this.#engineWorker = this.#createEngineWorker(mode);
      const engineReady = this.#workerReady(this.#engineWorker, ENGINE_CHANNEL, "engine");
      const nextReady = Promise.all([engineReady, renderAttached]).then(([engine, render]) => ({
        engine,
        render,
        transportMode: this.#transportMode,
        session: this.#session,
      }));
      this.#ready = nextReady;
      this.#postEngineInit(
        this.#engineWorker,
        channel.port1,
        mode,
        this.#sceneJson,
        this.#loopDurationSeconds,
        this.#session,
        this.#sceneSpecJson,
      );

      const ready = await nextReady;
      this.#assertLifecycleCurrent(generation);
      this.#playing = true;
      if (!wasPlaying) {
        const paused = await this.#requestEngine("pause", {});
        this.#rememberPlaying(paused);
        this.#assertLifecycleCurrent(generation);
      }
      if (callbacks !== null && authoringClient !== null) {
        this.#hostAuthoringClient = null;
        await this.#configureHostCallbacks(callbacks, authoringClient);
        this.#assertLifecycleCurrent(generation);
        await this.#requestEngine("request_callback_phase", {});
        this.#assertLifecycleCurrent(generation);
      }
      this.#fatalOwner = null;
      return ready;
    } catch (error) {
      if (generation !== this.#lifecycleGeneration) {
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      throw error;
    }
  }

  async #restartAll() {
    const mode = this.#mode;
    const sceneJson = this.#sceneJson;
    const sceneSpecJson = this.#sceneSpecJson;
    const loopDurationSeconds = this.#loopDurationSeconds;
    const transportMode = this.#transportMode;
    const sharedSlotCapacity = this.#sharedSlotCapacity;
    const wasPlaying = this.#playing;
    const callbacks = mode === EXECUTION_MODE_LEGACY ? this.#hostCallbacks : null;
    const authoringClient =
      mode === EXECUTION_MODE_LEGACY ? this.#hostAuthoringClient : null;
    const semanticAuthoringClient = this.#semanticAuthoringClient;
    const semanticContextId = this.#semanticContextId;
    const semanticCallbackSessionId = this.#semanticCallbackSessionId;

    if (this.#engineWorker !== null || this.#renderWorker !== null) {
      this.terminate({ preserveHostConfiguration: true });
      this.#canvas = replaceExecutionCanvas(this.#canvas);
    }

    const ready =
      mode === EXECUTION_MODE_SEMANTIC
        ? await this.startSemanticExecution(semanticContextId, semanticAuthoringClient, {
          loopDurationSeconds,
          transportMode,
          sharedSlotCapacity,
          callbackSessionId: semanticCallbackSessionId,
          initiallyPaused: !wasPlaying,
          })
        : await this.#startMode(
            mode,
            sceneJson,
            { loopDurationSeconds, transportMode, sharedSlotCapacity },
            sceneSpecJson,
          );
    if (!wasPlaying && mode !== EXECUTION_MODE_SEMANTIC) {
      const paused = await this.#requestEngine("pause", {});
      this.#rememberPlaying(paused);
    }
    if (callbacks !== null && authoringClient !== null) {
      this.#hostAuthoringClient = null;
      await this.#configureHostCallbacks(callbacks, authoringClient);
    }
    this.#fatalOwner = null;
    return ready;
  }

  terminate({ preserveHostConfiguration = false } = {}) {
    const restorePreparedCanvas =
      this.#engineWorker === null &&
      this.#renderWorker !== null &&
      this.#renderPrepared !== null;
    this.#lifecycleGeneration += 1;
    const cancellation = new Error(LIFECYCLE_CANCELLED_MESSAGE);
    this.#candidateEngineReject?.(cancellation);
    this.#candidateEngineReject = null;
    closeEndpoint(this.#candidateEngineWorker);
    this.#candidateEngineWorker = null;
    if (this.#mode === EXECUTION_MODE_SEMANTIC) {
      retireSemanticEndpoint(this.#engineWorker);
      if (!preserveHostConfiguration) {
        void releaseSemanticContext(
          this.#semanticAuthoringClient,
          this.#semanticContextId,
        );
      }
    } else {
      closeEndpoint(this.#engineWorker);
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
    if (restorePreparedCanvas) {
      this.#canvas = replaceExecutionCanvas(this.#canvas);
    }
    if (!preserveHostConfiguration) {
      this.#hostAuthoringClient = null;
      this.#hostCallbacks = null;
      this.#semanticAuthoringClient = null;
      this.#semanticContextId = null;
      this.#semanticCallbackSessionId = null;
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
        if (message.type === "host_callback_error") {
          this.#notifyRecoverableError(
            new Error(message.message || "host callback failed"),
            "host",
          );
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
    if (this.#mode === EXECUTION_MODE_SEMANTIC) {
      retireSemanticEndpoint(this.#engineWorker);
    } else {
      closeEndpoint(this.#engineWorker);
    }
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

  #createEngineWorker(mode) {
    if (mode === EXECUTION_MODE_RETAINED) {
      return new Worker(new URL("./retained-execution-engine-worker.js", import.meta.url), {
        type: "module",
        name: "noon-mixed-retained-engine",
      });
    }
    return new Worker(new URL("./execution-engine-worker.js", import.meta.url), {
      type: "module",
      name: "noon-engine",
    });
  }

  #postEngineInit(
    worker,
    port,
    mode,
    sceneJson,
    loopDurationSeconds,
    session = this.#session,
    sceneSpecJson = null,
  ) {
    const payload = {
      port,
      loopDurationSeconds,
      transportMode: this.#transportMode,
      sharedSlotCapacity: this.#sharedSlotCapacity,
      session,
    };
    if (mode === EXECUTION_MODE_RETAINED) {
      validateSceneSpecJson(sceneSpecJson);
      payload.sceneSpecJson = sceneSpecJson;
    } else {
      validateSceneJson(sceneJson);
      payload.sceneJson = sceneJson;
    }
    worker.postMessage(engineEnvelope("init", payload), [port]);
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

  #requireLegacyMode(operation) {
    this.#requireStarted();
    if (this.#mode !== EXECUTION_MODE_LEGACY) {
      throw new Error(`${operation} require legacy execution mode`);
    }
  }

  #requireSemanticMode(operation) {
    this.#requireStarted();
    if (this.#mode !== EXECUTION_MODE_SEMANTIC) {
      throw new Error(`${operation} requires semantic execution mode`);
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

function validateExecutionMode(mode) {
  if (
    mode !== EXECUTION_MODE_LEGACY &&
    mode !== EXECUTION_MODE_RETAINED &&
    mode !== EXECUTION_MODE_SEMANTIC
  ) {
    throw new TypeError(`unsupported execution mode ${mode}`);
  }
  return mode;
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

function validateSceneJson(sceneJson) {
  if (typeof sceneJson !== "string" || sceneJson.trim() === "") {
    throw new TypeError("scene must be non-empty JSON text");
  }
}

function validateSceneSpecJson(sceneSpecJson) {
  if (typeof sceneSpecJson !== "string" || sceneSpecJson.trim() === "") {
    throw new TypeError("canonical SceneSpec must be non-empty JSON text");
  }
  let sceneSpec;
  try {
    sceneSpec = JSON.parse(sceneSpecJson);
  } catch (error) {
    throw new TypeError(`canonical SceneSpec must be valid JSON: ${error}`);
  }
  if (!sceneSpec || typeof sceneSpec !== "object" || Array.isArray(sceneSpec)) {
    throw new TypeError("canonical SceneSpec must be an object");
  }
  if (sceneSpec.version !== SCENE_SPEC_VERSION) {
    throw new TypeError(`unsupported canonical SceneSpec version ${sceneSpec.version}`);
  }
  if (!Array.isArray(sceneSpec.objects) || !Array.isArray(sceneSpec.tracks)) {
    throw new TypeError("canonical SceneSpec must contain object and track arrays");
  }
  const objectIds = new Set();
  for (const object of sceneSpec.objects) {
    if (
      !object ||
      typeof object !== "object" ||
      Array.isArray(object) ||
      !Number.isSafeInteger(object.id) ||
      object.id < 0
    ) {
      throw new TypeError("canonical SceneSpec object has an invalid object ID");
    }
    if (objectIds.has(object.id)) {
      throw new TypeError("canonical SceneSpec has duplicate object IDs");
    }
    objectIds.add(object.id);
  }
  if (
    sceneSpec.camera_object !== null &&
    sceneSpec.camera_object !== undefined &&
    (!Number.isSafeInteger(sceneSpec.camera_object) ||
      sceneSpec.camera_object < 0 ||
      !objectIds.has(sceneSpec.camera_object))
  ) {
    throw new TypeError("canonical SceneSpec has an invalid camera object");
  }
  return sceneSpec;
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

function validateAuthoringClient(authoringClient) {
  if (!authoringClient || typeof authoringClient.attachEnginePort !== "function") {
    throw new TypeError("host callbacks require a PythonAuthoringClient");
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
