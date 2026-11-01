import {
  DEFAULT_SHARED_SLOT_CAPACITY,
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  selectExecutionTransportMode,
} from "./execution-transport.js";
import { replaceExecutionCanvas } from "./execution-canvas.js";
import { MainThreadRenderWorker } from "./main-thread-render-worker.js";
import { WorkerAudioDevice } from "./worker-audio-device.js";
import {
  RENDER_HOST_MAIN_THREAD,
  RENDER_HOST_WORKER,
  selectExecutionRenderHost,
} from "./render-host-selection.js";

const ENGINE_CHANNEL = "noon.engine";
const ENGINE_PROTOCOL_VERSION = 1;
// Host-owned timing on the existing correlated response, never wire authority.
const AUDIO_CLOCK_BRACKET = Symbol("noon-audio-window-bracket");
const RENDER_CHANNEL = "noon.render";
const RENDER_PROTOCOL_VERSION = 1;
const WORKER_OWNERS = Object.freeze(["engine", "render"]);
const RENDER_MODE_RETAINED = "retained";
const EXECUTION_MODE_SEMANTIC = "semantic";
const SEMANTIC_PACING_REALTIME = "realtime";
const SEMANTIC_PACING_EXTERNAL_SAMPLES = "external_samples";
// Producer reservations bound promises and messages even before ready() settles.
// The engine retains its own bounded control queue; this is not another queue.
export const MAX_IN_FLIGHT_NATIVE_INPUTS = 64;
const LIFECYCLE_CANCELLED_MESSAGE =
  "execution worker client was terminated during an asynchronous operation";

// Local lifecycle cancellation, never a remote callback/runtime error. Input
// callers still receive rejection; DOM collectors can retire without an alert.
export class ExecutionTransitionCancelled extends Error {
  constructor() {
    super("execution engine transitioning to semantic");
  }
}

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
  #nativeInputsInFlight = 0;
  #pointerView = null;
  #pointerViewRegistration = null;
  #pointerReceipt = null;
  #session = 0;
  #loopDurationSeconds = 4;
  #transportMode = null;
  #sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY;
  #ready = null;
  #playing = true;
  #audioDevice = new WorkerAudioDevice(() => this.#audioInterrupted());
  #audioControlGeneration = 0;
  // Explicit viewer gain supersedes older gain toggles, not an already
  // committed worker transport acknowledgement.
  #audioGainGeneration = 0;
  // Scene acknowledgements arrive in worker order. Retire older external
  // command completions without suppressing a later admitted scene action.
  #audioCommandGeneration = 0;
  #sceneControlGeneration = 0;
  #pendingSceneControl = null;
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
    const initialPointerView = options.initialPointerView === undefined ? undefined
      : validatedBrowserPointerView(options.initialPointerView.revision, options.initialPointerView.width, options.initialPointerView.height);
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
      initialPointerView,
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
    initialPointerView,
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
      // Install the exact mapping before any renderer receipt can arrive.
      // It authorizes nothing until the matching presentation is acknowledged.
      this.#pointerView = initialPointerView ?? null;
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
        { ...this.#semanticAttachmentOptions(
          loopDurationSeconds,
          this.#session,
          callbackSessionId,
          continuationGeneration,
          initiallyPaused,
          pacing,
        ), initialPointerView },
      );
      this.#ready = Promise.all([engineReady, renderReady, attached]).then(
        async ([engine, renderResult]) => {
          await this.#prepareAudio(engine);
          this.#assertLifecycleCurrent(generation);
          return { engine, render: renderResult,
            transportMode: this.#transportMode, session: this.#session };
        },
      );
      const ready = await this.#ready;
      this.#assertLifecycleCurrent(generation);
      this.#rememberPlaybackDuration(ready.engine, loopDurationSeconds);
      this.#renderPrepared = null;
      this.#semanticAuthoringClient = authoringClient;
      this.#semanticContextId = contextId;
      this.#semanticCallbackSessionId = callbackSessionId;
      this.#semanticPacing = pacing;
      this.#playing = ready.engine.playing ?? !initiallyPaused;
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

  async retireExecutionSource() {
    if (this.#semanticContextId === null) return;
    this.#requireStarted();
    const generation = this.#lifecycleGeneration;
    const engine = this.#engineWorker;
    const contextId = this.#semanticContextId;
    this.#audioControlGeneration += 1;
    this.#audioDevice.stop();
    // Await the canonical owner's stop + retirement, not a fire-and-forget
    // endpoint message. Rejection keeps that exact source available for retry.
    await this.#semanticAuthoringClient.releaseSemanticExecution(contextId);
    this.#assertLifecycleCurrent(generation);
    if (engine !== this.#engineWorker || contextId !== this.#semanticContextId) {
      throw new ExecutionTransitionCancelled();
    }
    closeEndpoint(engine);
    this.#engineWorker = null;
    this.#semanticAuthoringClient = null;
    this.#semanticContextId = null;
    this.#semanticCallbackSessionId = null;
    this.#ready = null;
    this.#playing = false;
    this.#pointerView = null;
    this.#pointerViewRegistration = null;
    this.#pointerReceipt = null;
    this.#rejectOwner("engine", new ExecutionTransitionCancelled());
    this.#retireAudio();
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
    // Explicit Run may have released its source before authoring in the same
    // store. The retained renderer/canvas can accept the ordinary new endpoint.
    if (this.#renderWorker === null || this.#session === 0) {
      throw new Error("ExecutionWorkerClient has not been started");
    }
    validateSemanticContextId(contextId);
    validateSemanticAuthoringClient(authoringClient);
    const generation = this.#lifecycleGeneration;
    const previousSession = this.#session;
    const previousSemanticAuthoringClient = this.#semanticAuthoringClient;
    const previousSemanticContextId = this.#semanticContextId;
    const wasPlaying = this.#playing;
    const duration = loopDurationSeconds ?? this.#loopDurationSeconds;
    const oldEngine = this.#engineWorker;
    this.#invalidateSceneControl();
    if (this.#audioDevice.available) {
      this.#audioControlGeneration += 1;
      this.#audioDevice.stop();
      this.#rememberPlaying(await this.#requestEngine("pause", {}));
    }
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

    const reconnectError = new ExecutionTransitionCancelled();
    this.#candidateEngineWorker = null;
    this.#engineWorker = candidate;
    this.#attachCurrentWorkerEvents(candidate, ENGINE_CHANNEL, "engine");
    this.#session = nextSession;
    // Equal platform dimensions still require registration in the new owner.
    // Neither a view nor a receipt is replayed across transport sessions.
    this.#pointerView = null;
    this.#pointerViewRegistration = null;
    this.#pointerReceipt = null;
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
      this.#ready = renderSwitched.then(async (renderResult) => {
        await this.#prepareAudio(candidateReady);
        this.#assertLifecycleCurrent(generation);
        return { engine: candidateReady, render: renderResult,
          transportMode: this.#transportMode, session: this.#session };
      });
      const ready = await this.#ready;
      this.#assertLifecycleCurrent(generation);
      this.#playing = candidateReady.playing ?? (continuationGeneration === null ? wasPlaying : true);
      this.#semanticAuthoringClient = authoringClient;
      this.#semanticContextId = contextId;
      this.#semanticCallbackSessionId = callbackSessionId;
      this.#rememberPlaybackDuration(candidateReady, duration);
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
    this.#invalidateSceneControl();
    const result = await this.#requestEngine("set_loop_duration", {
      loopDurationSeconds: duration,
    });
    this.#rememberPlaying(result);
    this.#loopDurationSeconds = duration;
    return result;
  }

  async pause() {
    return this.#audioCommand("pause", {});
  }

  async resume() {
    // Web Audio resume must be initiated in the click's activation turn.
    const unlocked = this.#audioDevice.unlock();
    return this.#audioCommand("resume", {}, unlocked);
  }

  async seek(timeSeconds) {
    const time = validateSeekTimeSeconds(timeSeconds, this.#loopDurationSeconds);
    return this.#audioCommand("seek", { time });
  }

  // Advance one canonical session barrier to an exact authored time. The
  // semantic endpoint owns forward progression and callback ordering; callers
  // receive only after the matching renderer publication has presented.
  async advanceTo(timeSeconds, { collectTimings = false, stopAtSourceCompletion = false } = {}) {
    this.#requireStarted();
    if (typeof stopAtSourceCompletion !== "boolean") {
      throw new TypeError("stopAtSourceCompletion must be a boolean");
    }
    const time = stopAtSourceCompletion
      ? validateAuthoredSampleTime(timeSeconds)
      : validateSeekTimeSeconds(timeSeconds, this.#loopDurationSeconds);
    if (typeof collectTimings !== "boolean") {
      throw new TypeError("collectTimings must be a boolean");
    }
    const result = await this.#requestEngine("advance_to", {
      time, ...(collectTimings ? { collectTimings: true } : {}),
      ...(stopAtSourceCompletion ? { stopAtSourceCompletion: true } : {}),
    });
    this.#rememberPlaying(result);
    return result;
  }

  async debugFrame() {
    this.#requireStarted();
    const result = await this.#requestEngine("debug_frame", {});
    return result.debugFrame;
  }

  async sampleToAuthoredTime(timeSeconds, { stopAtSourceCompletion = false, collectTimings = false } = {}) {
    this.#requireStarted();
    if (this.#semanticPacing !== SEMANTIC_PACING_EXTERNAL_SAMPLES) {
      throw new Error("external authored-time sampling requires external sample pacing");
    }
    const time = validateAuthoredSampleTime(timeSeconds);
    if (typeof stopAtSourceCompletion !== "boolean") {
      throw new TypeError("stopAtSourceCompletion must be a boolean");
    }
    if (typeof collectTimings !== "boolean") {
      throw new TypeError("collectTimings must be a boolean");
    }
    const result = await this.#requestEngine("sample_to_authored_time", {
      time, stopAtSourceCompletion, ...(collectTimings ? { collectTimings: true } : {}),
    });
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
    return this.#audioCommand("restart_playback", {});
  }

  async seekSection(previous = false) {
    if (typeof previous !== "boolean") throw new TypeError("section direction must be boolean");
    return this.#audioCommand("seek_section", { previous });
  }

  setAudioVolume(volume) {
    this.#audioDevice.setVolume(volume);
    this.#audioGainGeneration += 1;
    this.#invalidateSceneControl();
  }
  setAudioMuted(muted) {
    this.#audioDevice.setMuted(muted);
    this.#audioGainGeneration += 1;
    this.#invalidateSceneControl();
  }

  async beginFiniteExport(frameOptions) {
    this.#invalidateSceneControl();
    this.#audioControlGeneration += 1;
    this.#audioDevice.stop();
    const result = await this.#requestEngine("begin_finite_export", { frameOptions });
    this.#rememberPlaying(result);
    return result.finiteExport;
  }

  async sampleFiniteExport() {
    return (await this.#requestEngine("sample_finite_export", {})).finiteExport;
  }

  async acknowledgeFiniteExport(observation) {
    return (await this.#requestEngine("acknowledge_finite_export", { observation })).finiteExport;
  }

  async finiteExportAudioBlock(firstFrame, frameCount) {
    return (await this.#requestEngine("finite_export_audio", { firstFrame, frameCount })).audioBlock;
  }

  async cancelFiniteExport() {
    return this.#requestEngine("cancel_finite_export", {});
  }

  async #audioCommand(type, payload, unlocked = null) {
    this.#invalidateSceneControl();
    const generation = ++this.#audioControlGeneration;
    const commandGeneration = ++this.#audioCommandGeneration;
    const lifecycle = this.#lifecycleGeneration;
    const session = this.#session;
    this.#audioDevice.pause();
    if (unlocked !== null) await unlocked;
    if (generation !== this.#audioControlGeneration || commandGeneration !== this.#audioCommandGeneration ||
        lifecycle !== this.#lifecycleGeneration) {
      const error = new Error("audio playback command was superseded");
      error.name = "AbortError";
      throw error;
    }
    const result = await this.#requestEngine(type, payload);
    if (generation === this.#audioControlGeneration && commandGeneration === this.#audioCommandGeneration &&
        lifecycle === this.#lifecycleGeneration && session === this.#session) {
      try { this.#audioDevice.applyAnchor(result.audioAnchor, session, this.#audioBracket(result)); }
      catch (error) {
        this.#audioDevice.stop();
        await this.#requestEngine("pause", {});
        throw error;
      }
      this.#rememberPlaying(result);
    }
    return this.#audioState(result);
  }

  async undoTranslationDrag() {
    this.#requireStarted();
    return this.#requestEngine("undo_translation_drag", {});
  }

  // Forward one normalized semantic native-state sample to the canonical session.
  async setNativeStateInput(source, value) {
    this.#requireStarted();
    return this.#requestNativeInput("native_state_input", { source, value });
  }

  // Session/editor policy, ordered with pointer input and never auto-replayed.
  async setPointerFillSelection(maxMovement) {
    this.#requireStarted();
    if (maxMovement !== null &&
        (typeof maxMovement !== "number" || !Number.isFinite(maxMovement) || maxMovement < 0)) {
      throw new TypeError("selection tolerance must be a finite nonnegative number or null");
    }
    return this.#requestNativeInput("pointer_fill_selection", { maxMovement });
  }

  // Platform view registration is not proof of presentation. Retire the locally
  // observed receipt synchronously, before registration or input can yield.
  setBrowserPointerView(revision, width, height) {
    this.#requireStarted();
    const view = validatedBrowserPointerView(revision, width, height);
    if (this.#pointerView !== null && revision < this.#pointerView.revision) {
      throw new TypeError("invalid or retired browser pointer view");
    }
    if (this.#pointerView?.revision === revision) {
      if (this.#pointerView.width !== width || this.#pointerView.height !== height) {
        throw new TypeError("browser pointer view revision reused with different geometry");
      }
      if (this.#pointerViewRegistration?.view === this.#pointerView) {
        return this.#pointerViewRegistration.delivery;
      }
      return Promise.resolve({ type: "browser_pointer_view" });
    }
    this.#invalidateSceneControl();
    this.#pointerView = view;
    this.#pointerReceipt = null;
    const requested = this.#requestNativeInput("browser_pointer_view", { view });
    let delivery;
    delivery = requested.then(
      result => {
        if (this.#pointerViewRegistration?.delivery === delivery) {
          this.#pointerViewRegistration = null;
        }
        return result;
      },
      error => {
        if (this.#pointerViewRegistration?.delivery === delivery) {
          this.#pointerViewRegistration = null;
          if (this.#pointerView === view) {
            // The DOM mapping changed, so never restore the old receipt. Leave
            // registration absent so the exact same revision can be retried.
            this.#pointerView = null;
            this.#pointerReceipt = null;
          }
        }
        throw error;
      },
    );
    this.#pointerViewRegistration = { view, delivery };
    return delivery;
  }

  get pointerPresentation() {
    return this.#pointerReceipt?.session === this.#session &&
      this.#pointerReceipt.view_revision === this.#pointerView?.revision &&
      this.#candidateEngineWorker === null && this.#fatalOwner === null
      ? this.#pointerReceipt : null;
  }

  // Pin the last observed successful presentation at collection, before readiness
  // or delivery yields. A later acknowledgement never relabels this occurrence.
  async submitBrowserPointerInput(input) {
    this.#requireStarted();
    if (["press", "cancel", "capture_lost", "focus_lost"].includes(input.kind)) {
      this.#invalidateSceneControl();
    }
    const admission = {
      generation: this.#sceneControlGeneration,
      audioGeneration: this.#audioControlGeneration,
      gainGeneration: this.#audioGainGeneration,
      lifecycle: this.#lifecycleGeneration, session: this.#session,
      worker: this.#engineWorker,
    };
    // Browser activation expires before a worker round trip. Unlock is silent;
    // only the shared Rust click action can subsequently admit playback. Keep a
    // rejected unlock observed even when this release hits no playback control.
    let unlocked = null;
    if (input.kind === "release" && input.button === 0) {
      try { unlocked = Promise.resolve(this.#audioDevice.unlock()).then(() => null, error => error); }
      catch (error) { unlocked = Promise.resolve(error); }
    }
    const result = await this.#requestNativeInput("browser_pointer_input", {
      input, presentation: this.pointerPresentation,
    });
    if (result.pointerInputAccepted === false) this.#invalidateSceneControl();
    if (!this.#sceneControlOwnerCurrent(admission)) {
      const sequence = result.sceneControl?.sequence;
      if (typeof sequence === "string" && /^(0|[1-9][0-9]*)$/.test(sequence) &&
          sequence.length <= 20 && BigInt(sequence) <= 0xffffffffffffffffn) {
        this.#cancelSceneControlReceipt({ ...admission, sequence });
        return { ...result, sceneControlApplied: false };
      }
      return result;
    }
    if (result.sceneControlApplied === true) {
      this.#invalidateSceneControl();
      if (result.sceneControlAction === "toggle_audio_muted" &&
          admission.gainGeneration !== this.#audioGainGeneration) {
        return this.#audioState({ ...result, sceneControlApplied: false });
      }
      return this.#applySceneControlResult(result);
    }
    if (result.sceneControl === undefined) return result;
    const { sequence, action } = result.sceneControl;
    if (typeof sequence !== "string" || !/^(0|[1-9][0-9]*)$/.test(sequence) ||
        sequence.length > 20 || BigInt(sequence) > 0xffffffffffffffffn ||
        action !== "toggle_playback" || result.playing !== false || unlocked === null) {
      throw new Error("invalid pending worker scene playback control");
    }
    if (admission.generation !== this.#sceneControlGeneration) {
      this.#cancelSceneControlReceipt({ ...admission, sequence });
      return { ...result, sceneControlApplied: false };
    }
    this.#invalidateSceneControl();
    this.#audioCommandGeneration += 1;
    let cancel;
    const cancelled = new Promise(resolve => { cancel = () => resolve(null); });
    const pending = { ...admission, sequence, generation: this.#sceneControlGeneration, cancel };
    this.#pendingSceneControl = pending;
    const unlockError = await Promise.race([unlocked, cancelled]);
    if (this.#pendingSceneControl !== pending || !this.#sceneControlOwnerCurrent(pending)) {
      return { ...result, sceneControlApplied: false };
    }
    if (unlockError !== null) {
      this.#invalidateSceneControl();
      throw unlockError;
    }
    try {
      const completed = await this.#requestNativeInput("scene_control", { sequence });
      // Once sent, the worker's ordered result decides whether Play committed.
      // A later press or gain change may retire an unconsumed receipt but cannot
      // undo a transport action the worker has already applied.
      if (!this.#sceneControlOwnerCurrent(pending)) {
        return { ...result, sceneControlApplied: false };
      }
      if (this.#pendingSceneControl === pending) this.#pendingSceneControl = null;
      return this.#applySceneControlResult({ ...result, ...completed,
        type: result.type, pointerInputAccepted: result.pointerInputAccepted },
        completed[AUDIO_CLOCK_BRACKET]);
    } finally {
      if (this.#pendingSceneControl === pending) this.#pendingSceneControl = null;
    }
  }

  #sceneControlOwnerCurrent(receipt) {
    return receipt.lifecycle === this.#lifecycleGeneration && receipt.session === this.#session &&
      receipt.worker === this.#engineWorker && receipt.audioGeneration === this.#audioControlGeneration &&
      this.#candidateEngineWorker === null && this.#fatalOwner === null;
  }

  #cancelSceneControlReceipt(receipt) {
    if (receipt.lifecycle !== this.#lifecycleGeneration || receipt.session !== this.#session ||
        receipt.worker !== this.#engineWorker || this.#candidateEngineWorker !== null || this.#fatalOwner !== null) return;
    // One bounded best-effort retirement, qualified by the original sequence.
    // A failed transport cannot authorize a later completion or cancel a newer
    // pending action. Normal worker faults already use the client's error path.
    void this.#requestNativeInput("scene_control", { sequence: receipt.sequence, cancel: true }).catch(() => {});
  }

  #invalidateSceneControl() {
    const pending = this.#pendingSceneControl;
    this.#pendingSceneControl = null;
    this.#sceneControlGeneration += 1;
    if (pending !== null) {
      pending.cancel();
      this.#cancelSceneControlReceipt(pending);
    }
  }

  async #applySceneControlResult(result, bracket = result?.[AUDIO_CLOCK_BRACKET] ?? null) {
    if (result.sceneControlApplied !== true) return this.#audioState(result);
    this.#audioCommandGeneration += 1;
    if (result.sceneControlAction === "toggle_audio_muted") {
      this.#audioDevice.setMuted(!this.#audioDevice.muted);
    } else {
      try { this.#audioDevice.applyAnchor(result.audioAnchor, this.#session, this.#audioBracket(result, bracket)); }
      catch (error) {
        this.#audioDevice.stop();
        await this.#requestEngine("pause", {});
        throw error;
      }
    }
    this.#rememberPlaying(result);
    return this.#audioState(result);
  }

  #audioBracket(result, bracket = result?.[AUDIO_CLOCK_BRACKET]) {
    if (result.audioAnchor?.playing !== true) return null;
    if (!bracket || bracket.worker !== this.#engineWorker ||
        bracket.lifecycle !== this.#lifecycleGeneration || bracket.session !== this.#session ||
        bracket.session !== result.audioAnchor.session) {
      throw new Error("playing worker audio anchor requires its current correlated request");
    }
    return bracket;
  }

  // View navigation is occurrence-bound input too. The receipt and normalized
  // cursor are copied by the existing bounded ingress before any readiness yield.
  async scrollInspectionView(input) {
    this.#requireStarted();
    this.#invalidateSceneControl();
    return this.#requestNativeInput("inspection_scroll", {
      input, presentation: this.pointerPresentation,
    });
  }

  // Forward one normalized semantic native-event source to the canonical session.
  async emitNativeEvent(source) {
    this.#requireStarted();
    return this.#requestNativeInput("native_event", { source });
  }

  async state() {
    const result = await this.#requestEngine("state", {});
    this.#rememberPlaying(result);
    return this.#audioState(result);
  }

  async metrics({ profilePublicationStages = false, profileRenderSubstages = false,
    profileGpuCompletion = false, includeGpuIdentity = false } = {}) {
    if (typeof profilePublicationStages !== "boolean" ||
        typeof profileRenderSubstages !== "boolean" || typeof profileGpuCompletion !== "boolean" ||
        typeof includeGpuIdentity !== "boolean") {
      throw new TypeError("renderer profiling options must be booleans");
    }
    const [render, engine] = await Promise.all([
      this.#requestRender("metrics", { profilePublicationStages, profileRenderSubstages,
        profileGpuCompletion, includeGpuIdentity }),
      this.#engineWorker === null ? null : this.#requestEngine("metrics", {}),
    ]);
    return { ...render, engineMetrics: engine?.metrics ?? null, renderHost: this.#renderHost };
  }

  resize(width, height, devicePixelRatio = 1) {
    if (this.#renderWorker === null) {
      throw new Error("ExecutionWorkerClient has no retained renderer");
    }
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
    this.#retireAudio();
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
    this.#pointerReceipt = null;
    this.#pointerView = null;
    this.#pointerViewRegistration = null;
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

  // Input is occurrence-bound: reserve before yielding, snapshot at admission,
  // and never migrate a delayed delivery to a replacement endpoint. A rejected
  // acknowledgement is not permission to repeat an externally observed edge.
  async #requestNativeInput(type, payload) {
    this.#requireStarted();
    if (this.#candidateEngineWorker !== null || this.#fatalOwner !== null) {
      throw new Error("native input requires a stable, non-transitioning execution endpoint");
    }
    if (this.#nativeInputsInFlight >= MAX_IN_FLIGHT_NATIVE_INPUTS) {
      throw new Error("native input in-flight capacity is full; wait for acknowledgement before retrying");
    }
    const worker = this.#engineWorker;
    const ready = this.#ready;
    const generation = this.#lifecycleGeneration;
    this.#nativeInputsInFlight += 1;
    try {
      // This copy belongs to the genuine worker boundary, not the Rust engine.
      const snapshot = structuredClone(payload);
      await ready;
      if (worker !== this.#engineWorker || ready !== this.#ready ||
          generation !== this.#lifecycleGeneration ||
          this.#candidateEngineWorker !== null || this.#fatalOwner !== null) {
        throw new Error("native input belongs to a retired or transitioning execution endpoint");
      }
      return await this.#request(worker, "engine", engineEnvelope, type, snapshot);
    } finally {
      // Do not reset this counter on restart: old reservations still own their
      // releases, and cannot decrement a new generation's independent counter.
      this.#nativeInputsInFlight -= 1;
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
    if (this.#ready !== null) {
      await this.ready();
    } else if (this.#renderWorker === null || this.#session === 0) {
      throw new Error("ExecutionWorkerClient has not been started");
    }
    // Successful source retirement releases Runtime ownership, while the
    // retained renderer still owns its last picture and diagnostic counters.
    // Query those actual counters without inventing a replacement engine.
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
    // A sample taken in the matching worker request must lie
    // between this Window send and its correlated Window response receipt.
    // Keep the observation on the existing bounded pending-request entry.
    const observeAudio = owner === "engine" &&
      (["pause", "resume", "seek", "seek_section", "restart_playback", "scene_control"].includes(type) ||
        (type === "browser_pointer_input" && payload?.input?.kind === "release"));
    return new Promise((resolve, reject) => {
      const key = `${owner}:${requestId}`;
      const pending = { resolve, reject };
      this.#pending.set(key, pending);
      try {
        const envelope = envelopeFactory(type, { ...payload, requestId });
        if (observeAudio) {
          pending.audioClock = { sentMs: performance.now(), session: this.#session,
            lifecycle: this.#lifecycleGeneration, worker, requestId, type };
        }
        worker.postMessage(envelope, transfer);
      } catch (error) {
        this.#pending.delete(key);
        reject(error);
      }
    });
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
        if (owner === "engine" && message.type === "audio_stopped") {
          this.#invalidateSceneControl();
          this.#audioControlGeneration += 1;
          this.#audioDevice.stop();
          return;
        }
        if (owner === "engine" && message.type === "audio_state") {
          // Unsolicited anchors can only retire naturally completed output.
          if (message.audioAnchor?.session !== this.#session || message.audioAnchor?.playing !== false) {
            throw new Error("invalid terminal worker audio state");
          }
          this.#audioDevice.pause();
          this.#playing = false;
          return;
        }
        if (owner === "engine" && message.type === "pointer_presented") {
          const receipt = message.receipt;
          if (!receipt || ![receipt.session, receipt.sequence, receipt.presentation, receipt.view_revision]
              .every(value => Number.isSafeInteger(value) && value >= 0) || receipt.presentation === 0) {
            throw new Error("invalid worker pointer presentation receipt");
          }
          if (receipt.session === this.#session && receipt.view_revision === this.#pointerView?.revision &&
              (this.#pointerReceipt === null || receipt.presentation >= this.#pointerReceipt.presentation)) {
            this.#pointerReceipt = Object.freeze({ session: receipt.session, sequence: receipt.sequence,
              presentation: receipt.presentation, view_revision: receipt.view_revision });
          }
          return;
        }
        if (owner === "engine" && message.type === "pointer_presentation_invalidated") {
          const current = this.#pointerReceipt, receipt = message.receipt;
          if (current !== null && receipt && current.session === receipt.session &&
              current.sequence === receipt.sequence && current.presentation === receipt.presentation &&
              current.view_revision === receipt.view_revision) {
            this.#invalidateSceneControl();
            this.#pointerReceipt = null;
          }
          return;
        }
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
          this.#settle(owner, message.requestId, ({ resolve: resolvePending, reject: rejectPending, audioClock }) => {
            if (audioClock && message.type !== audioClock.type) {
              rejectPending(new Error("worker audio response does not match its issued command"));
              return;
            }
            if (audioClock && message.audioAnchor?.playing === true) {
              Object.defineProperty(message, AUDIO_CLOCK_BRACKET, {
                value: Object.freeze(audioClock), enumerable: false,
              });
            }
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
    // One Window reading at the authenticated matched response, not at a
    // later Promise continuation. No worker timestamp comparison is made.
    if (pending.audioClock) pending.audioClock.receivedMs = performance.now();
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
    this.#retireAudio();
    retireSemanticEndpoint(this.#engineWorker);
    this.#renderWorker?.terminate();
    this.#engineWorker = null;
    this.#renderWorker = null;
    this.#pointerReceipt = null;
    this.#pointerView = null;
    this.#pointerViewRegistration = null;
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
    this.#retireAudio();
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

  #rememberPlaybackDuration(engine, requestedDuration) {
    // Rust admits the finite endpoint. This is only the client's seek bound;
    // audio-device availability cannot change it. Ordinary looping playback
    // without a finite endpoint retains its explicitly requested duration.
    this.#loopDurationSeconds = Number.isFinite(engine.durationSeconds) && engine.durationSeconds > 0
      ? engine.durationSeconds : requestedDuration;
  }

  async #prepareAudio(engine) {
    globalThis.document?.removeEventListener?.("visibilitychange", this.#visibilityChanged);
    globalThis.window?.removeEventListener?.("pagehide", this.#audioInterrupted);
    await this.#audioDevice.install(engine.audioResources);
    if (!this.#audioDevice.available) return;
    document.addEventListener("visibilitychange", this.#visibilityChanged);
    window.addEventListener("pagehide", this.#audioInterrupted);
  }

  #audioState(result) {
    return this.#audioDevice.available ? { ...result,
      audioReady: this.#audioDevice.ready, audioVolume: this.#audioDevice.volume,
      audioMuted: this.#audioDevice.muted } : result;
  }

  #retireAudio() {
    this.#invalidateSceneControl();
    this.#audioControlGeneration += 1;
    this.#audioDevice.dispose();
    globalThis.document?.removeEventListener?.("visibilitychange", this.#visibilityChanged);
    globalThis.window?.removeEventListener?.("pagehide", this.#audioInterrupted);
  }

  #visibilityChanged = () => {
    if (document.hidden) this.#audioInterrupted();
  };

  #audioInterrupted = () => {
    this.#invalidateSceneControl();
    this.#audioControlGeneration += 1;
    this.#audioDevice.stop();
    // The worker may have admitted resume while its picture/acknowledgement is
    // still in flight. A cached paused observation cannot authorize a later
    // hidden-tab restart; queue pause behind that same existing command stream.
    if (this.#engineWorker === null) return;
    void this.#requestEngine("pause", {}).then(result => this.#rememberPlaying(result))
      .catch(error => { if (this.#engineWorker !== null) this.#notifyError(error, "engine"); });
  };

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
    ...payload,
    channel: ENGINE_CHANNEL,
    protocolVersion: ENGINE_PROTOCOL_VERSION,
    type,
  };
}

function renderEnvelope(type, payload = {}) {
  return {
    ...payload,
    channel: RENDER_CHANNEL,
    protocolVersion: RENDER_PROTOCOL_VERSION,
    type,
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

function validatedBrowserPointerView(revision, width, height) {
  if (!Number.isSafeInteger(revision) || revision < 0 || !Number.isFinite(width) ||
      !Number.isFinite(height) || width < 0 || height < 0) {
    throw new TypeError("invalid or retired browser pointer view");
  }
  return Object.freeze({ revision, width, height });
}
