import { ExecutionWorkerClient } from "./execution-worker-client.js";

export const AUTHORING_EXECUTION_LEGACY = "legacy";
export const AUTHORING_EXECUTION_RETAINED = "retained";
export const AUTHORING_EXECUTION_SEMANTIC = "semantic";

const SCENE_SPEC_VERSION = 1;
const DEFAULT_LOOP_DURATION_SECONDS = 4;
const DEFAULT_SHARED_SLOT_CAPACITY = 1024 * 1024;
const LIFECYCLE_CANCELLED_MESSAGE =
  "AuthoringExecutionClient was terminated during an asynchronous operation";
const EMPTY_HOST_METRICS = Object.freeze({
  enabled: false,
  missedDeadlines: 0,
  droppedLateResults: 0,
});

/// One browser execution owner for Python authoring output.
///
/// The execution client owns one render worker and one transferred OffscreenCanvas
/// for its lifetime. Geometry-only results keep a legacy engine attached to that
/// owner. Results with canonical retained SceneSpec output switch only the engine
/// type and renderer implementation at an authoring boundary; the render worker,
/// HTML canvas, and OffscreenCanvas ownership remain stable. Retained edits rebuild
/// engine state at authoring boundaries; playback itself remains retained with no
/// per-frame Python/source work.
export class AuthoringExecutionClient {
  #canvas;
  #player = null;
  #preparedPlayer = null;
  #mode = null;
  #rendererBackend = "";
  #loopDurationSeconds = DEFAULT_LOOP_DURATION_SECONDS;
  #transportMode = null;
  #sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY;
  #onError;
  #onRecoverableError;
  #resizeObserver = null;
  #transition = null;
  #lifecycleGeneration = 0;

  constructor(canvas, { onError = null, onRecoverableError = null } = {}) {
    if (!(canvas instanceof HTMLCanvasElement)) {
      throw new TypeError("AuthoringExecutionClient requires an HTMLCanvasElement");
    }
    if (onError !== null && typeof onError !== "function") {
      throw new TypeError("AuthoringExecutionClient onError must be a function");
    }
    if (onRecoverableError !== null && typeof onRecoverableError !== "function") {
      throw new TypeError("AuthoringExecutionClient onRecoverableError must be a function");
    }
    this.#canvas = canvas;
    this.#onError = onError;
    this.#onRecoverableError = onRecoverableError;
    this.#observeCanvas();
  }

  get canvas() {
    return this.#canvas;
  }

  get mode() {
    return this.#mode;
  }

  get rendererBackend() {
    return this.#rendererBackend;
  }

  get transportMode() {
    return this.#transportMode;
  }

  async prepare(
    {
      transportMode = undefined,
      sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY,
    } = {},
  ) {
    if (this.#player !== null || this.#preparedPlayer !== null || this.#transition !== null) {
      throw new Error("AuthoringExecutionClient is already started or preparing");
    }
    this.#sharedSlotCapacity = validateSharedSlotCapacity(sharedSlotCapacity);
    const options = { sharedSlotCapacity: this.#sharedSlotCapacity };
    if (transportMode !== undefined) {
      options.transportMode = transportMode;
    }

    const generation = this.#lifecycleGeneration;
    const player = this.#createPlayer();
    this.#preparedPlayer = player;
    try {
      const ready = await player.prepare(options);
      this.#assertLifecycleCurrent(generation);
      if (this.#preparedPlayer !== player && this.#player !== player) {
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      this.#transportMode = ready.transportMode;
      return ready;
    } catch (error) {
      if (this.#preparedPlayer === player) {
        this.#preparedPlayer = null;
      }
      if (generation === this.#lifecycleGeneration) {
        this.#adoptPlayerCanvas(player);
      }
      if (generation !== this.#lifecycleGeneration) {
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      throw error;
    }
  }

  async start(
    sceneJson,
    {
      loopDurationSeconds = DEFAULT_LOOP_DURATION_SECONDS,
      transportMode = undefined,
      sharedSlotCapacity = undefined,
      callbacks = null,
      authoringClient = null,
    } = {},
  ) {
    if (this.#player !== null || this.#transition !== null) {
      throw new Error("AuthoringExecutionClient is already started");
    }
    validateSceneJson(sceneJson);
    this.#loopDurationSeconds = validateLoopDurationSeconds(loopDurationSeconds);
    this.#sharedSlotCapacity = this.#resolveStartupSharedSlotCapacity(sharedSlotCapacity);
    const options = {
      loopDurationSeconds: this.#loopDurationSeconds,
      sharedSlotCapacity: this.#sharedSlotCapacity,
    };
    if (transportMode !== undefined) {
      options.transportMode = transportMode;
    }
    const ready = await this.#startMode(AUTHORING_EXECUTION_LEGACY, sceneJson, options);
    if (callbacks !== null && callbacks !== undefined) {
      await this.#player.configureHostCallbacks(callbacks, authoringClient);
    }
    this.#transportMode = ready.transportMode;
    return ready;
  }

  async startRetainedCanonical(
    sceneSpecJson,
    {
      loopDurationSeconds = DEFAULT_LOOP_DURATION_SECONDS,
      transportMode = undefined,
      sharedSlotCapacity = undefined,
    } = {},
  ) {
    if (this.#player !== null || this.#transition !== null) {
      throw new Error("AuthoringExecutionClient is already started");
    }
    validateSceneSpecJson(sceneSpecJson);
    this.#loopDurationSeconds = validateLoopDurationSeconds(loopDurationSeconds);
    this.#sharedSlotCapacity = this.#resolveStartupSharedSlotCapacity(sharedSlotCapacity);
    const options = {
      loopDurationSeconds: this.#loopDurationSeconds,
      sharedSlotCapacity: this.#sharedSlotCapacity,
    };
    if (transportMode !== undefined) {
      options.transportMode = transportMode;
    }
    const ready = await this.#startMode(
      AUTHORING_EXECUTION_RETAINED,
      null,
      options,
      sceneSpecJson,
    );
    this.#transportMode = ready.transportMode;
    return ready;
  }

  async startSemanticExecution(
    descriptor,
    {
      authoringClient,
      loopDurationSeconds = DEFAULT_LOOP_DURATION_SECONDS,
      transportMode = undefined,
      sharedSlotCapacity = undefined,
      initiallyPaused = false,
    } = {},
  ) {
    if (this.#player !== null || this.#transition !== null) {
      throw new Error("AuthoringExecutionClient is already started");
    }
    const semantic = validateSemanticExecutionDescriptor(descriptor);
    validateSemanticAuthoringClient(authoringClient);
    if (typeof initiallyPaused !== "boolean") {
      throw new TypeError("initiallyPaused must be a boolean");
    }
    if (initiallyPaused && semantic.continuationGeneration !== null) {
      throw new Error("source-owned semantic continuations cannot start paused");
    }
    this.#loopDurationSeconds = validateLoopDurationSeconds(loopDurationSeconds);
    this.#sharedSlotCapacity = this.#resolveStartupSharedSlotCapacity(sharedSlotCapacity);
    const options = {
      loopDurationSeconds: this.#loopDurationSeconds,
      sharedSlotCapacity: this.#sharedSlotCapacity,
      initiallyPaused,
    };
    if (transportMode !== undefined) {
      options.transportMode = transportMode;
    }
    const generation = this.#lifecycleGeneration;
    const player = this.#preparedPlayer ?? this.#createPlayer();
    const terminateCandidate = createIdempotentTerminator(player);
    try {
      const ready = await player.startSemanticExecution(semantic.contextId, authoringClient, {
        ...options,
        callbackSessionId: semantic.callbackSessionId,
        continuationGeneration: semantic.continuationGeneration,
      });
      this.#assertLifecycleCurrent(generation, terminateCandidate);
      if (this.#preparedPlayer === player) {
        this.#preparedPlayer = null;
      }
      this.#player = player;
      this.#mode = AUTHORING_EXECUTION_SEMANTIC;
      this.#rendererBackend = ready.render.backend;
      this.#transportMode = ready.transportMode;
      this.#resizeCurrentCanvas();
      return ready;
    } catch (error) {
      if (this.#preparedPlayer === player) {
        this.#preparedPlayer = null;
      }
      terminateCandidate();
      if (generation === this.#lifecycleGeneration) {
        this.#adoptPlayerCanvas(player);
      }
      if (generation !== this.#lifecycleGeneration) {
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      throw error;
    }
  }

  async reconcileSemanticExecution(
    descriptor,
    { authoringClient, loopDurationSeconds = null } = {},
  ) {
    if (this.#transition !== null) {
      await this.#transition;
    }
    this.#requireStarted();
    const semantic = validateSemanticExecutionDescriptor(descriptor);
    validateSemanticAuthoringClient(authoringClient);
    const duration = validateOptionalLoopDurationSeconds(loopDurationSeconds);
    if (duration !== null) {
      this.#loopDurationSeconds = duration;
    }
    return this.#runTransition(async () => {
      const ready = await this.#player.switchToSemanticExecution(semantic.contextId, authoringClient, {
        loopDurationSeconds: duration,
        callbackSessionId: semantic.callbackSessionId,
      });
      this.#mode = AUTHORING_EXECUTION_SEMANTIC;
      this.#rendererBackend = ready.render.backend;
      this.#resizeCurrentCanvas();
      const state = await this.#player.state();
      return {
        type: "result",
        operation: "rebuild_semantic_execution",
        incremental: false,
        rebuilt: true,
        mode: this.#mode,
        ready,
        ...state,
      };
    });
  }

  async reconcileScene(
    sceneJson,
    {
      sceneSpecJson = null,
      retainedDocumentJson = null,
      callbacks = null,
      authoringClient = null,
      loopDurationSeconds = null,
    } = {},
  ) {
    if (this.#transition !== null) {
      await this.#transition;
    }
    this.#requireStarted();
    validateSceneJson(sceneJson);
    if (retainedDocumentJson !== null && retainedDocumentJson !== undefined) {
      throw new Error(
        "split retained reconciliation is retired; provide canonical sceneSpecJson instead",
      );
    }
    const duration = validateOptionalLoopDurationSeconds(loopDurationSeconds);
    if (duration !== null) {
      this.#loopDurationSeconds = duration;
    }

    if (sceneSpecJson !== null && sceneSpecJson !== undefined) {
      validateSceneSpecJson(sceneSpecJson);
      if (callbacks !== null && callbacks !== undefined) {
        throw new Error(
          "retained authoring with Python host callbacks is not supported yet; " +
            "split the callback work from retained text instead of silently dropping either",
        );
      }
      // Semantic execution already uses the retained mixed renderer. Rebuild
      // its resource bundle in place rather than asking the renderer to switch
      // from retained mode to itself.
      if (
        this.#mode === AUTHORING_EXECUTION_RETAINED ||
        this.#mode === AUTHORING_EXECUTION_SEMANTIC
      ) {
        return this.#runTransition(() => this.#rebuildRetainedCanonical(sceneSpecJson));
      }
      return this.#runTransition(() => this.#switchRetainedCanonical(sceneSpecJson));
    }

    if (this.#mode === AUTHORING_EXECUTION_LEGACY) {
      const result = await this.#player.reconcileScene(sceneJson, {
        callbacks,
        authoringClient,
        loopDurationSeconds: duration,
      });
      return { ...result, mode: this.#mode, rebuilt: false };
    }

    return this.#runTransition(() =>
      this.#switchLegacy(sceneJson, { callbacks, authoringClient }),
    );
  }

  async state() {
    return this.#withStablePlayer((player) => player.state());
  }

  async metrics() {
    const report = await this.#withStablePlayer((player) => player.metrics());
    return {
      ...report,
      executionMode: this.#mode,
      engineMetrics: {
        ...(report.engineMetrics ?? {}),
        host: report.engineMetrics?.host ?? EMPTY_HOST_METRICS,
      },
    };
  }

  async setLoopDurationSeconds(loopDurationSeconds) {
    const duration = validateLoopDurationSeconds(loopDurationSeconds);
    const result = await this.#withStablePlayer((player) =>
      player.setLoopDurationSeconds(duration),
    );
    this.#loopDurationSeconds = duration;
    return result;
  }

  async pause() {
    return this.#withStablePlayer((player) => player.pause());
  }

  async resume() {
    return this.#withStablePlayer((player) => player.resume());
  }

  async seek(timeSeconds) {
    return this.#withStablePlayer((player) => player.seek(timeSeconds));
  }

  async advanceTo(timeSeconds) {
    return this.#withStablePlayer((player, mode) => {
      if (mode !== AUTHORING_EXECUTION_SEMANTIC) {
        throw new Error("forward authored-time advancement requires semantic execution mode");
      }
      return player.advanceTo(timeSeconds);
    });
  }

  async advanceToWithRendererObservation(timeSeconds) {
    return this.#withStablePlayer((player, mode) => {
      if (mode !== AUTHORING_EXECUTION_SEMANTIC) {
        throw new Error("callback renderer observation requires semantic execution mode");
      }
      return player.advanceToWithRendererObservation(timeSeconds);
    });
  }

  async setNativeStateInput(source, value) {
    return this.#withStablePlayer((player, mode) => {
      if (mode !== AUTHORING_EXECUTION_SEMANTIC) {
        throw new Error("native state input requires semantic execution mode");
      }
      return player.setNativeStateInput(source, value);
    });
  }

  async emitNativeEvent(source) {
    return this.#withStablePlayer((player, mode) => {
      if (mode !== AUTHORING_EXECUTION_SEMANTIC) {
        throw new Error("native event input requires semantic execution mode");
      }
      return player.emitNativeEvent(source);
    });
  }

  async restartPlayback() {
    return this.#withStablePlayer((player) => player.restartPlayback());
  }

  async restart() {
    return this.#withStablePlayer(async (player, mode) => {
      const generation = this.#lifecycleGeneration;
      try {
        const ready = await player.restart();
        this.#assertLifecycleCurrent(generation);
        this.#canvas = player.canvas;
        this.#mode = mode;
        this.#rendererBackend = ready.render.backend;
        this.#transportMode = ready.transportMode;
        this.#observeCanvas();
        this.#resizeCurrentCanvas();
        return { ...ready, mode };
      } catch (error) {
        if (generation !== this.#lifecycleGeneration) {
          throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
        }
        this.#adoptPlayerCanvas(player);
        throw error;
      }
    });
  }

  async applyPatchBatch(patchBatchJson) {
    return this.#withStablePlayer((player, mode) => {
      if (mode !== AUTHORING_EXECUTION_LEGACY) {
        throw new Error("patch batches are not supported by this execution mode");
      }
      return player.applyPatchBatch(patchBatchJson);
    });
  }

  resize(width, height, devicePixelRatio = 1) {
    if (this.#player === null) {
      if (this.#transition !== null) {
        return;
      }
      this.#requireStarted();
    }
    this.#player.resize(width, height, devicePixelRatio);
  }

  terminate() {
    this.#lifecycleGeneration += 1;
    this.#resizeObserver?.disconnect();
    this.#resizeObserver = null;
    const preparedPlayer = this.#preparedPlayer;
    preparedPlayer?.terminate();
    if (preparedPlayer !== null && this.#canvas !== preparedPlayer.canvas) {
      this.#canvas = preparedPlayer.canvas;
    }
    this.#preparedPlayer = null;
    this.#player?.terminate();
    this.#player = null;
    this.#mode = null;
    this.#rendererBackend = "";
    this.#transportMode = null;
  }

  async #startMode(mode, sceneJson, options, sceneSpecJson = null) {
    const generation = this.#lifecycleGeneration;
    const player = this.#preparedPlayer ?? this.#createPlayer();
    const terminateCandidate = createIdempotentTerminator(player);
    let published = false;
    try {
      const ready =
        mode === AUTHORING_EXECUTION_RETAINED
          ? await player.startRetainedCanonical(sceneSpecJson, options)
          : await player.start(sceneJson, options);
      this.#assertLifecycleCurrent(generation, terminateCandidate);
      if (this.#preparedPlayer === player) {
        this.#preparedPlayer = null;
      }
      this.#player = player;
      published = true;
      this.#mode = mode;
      this.#rendererBackend = ready.render.backend;
      this.#resizeCurrentCanvas();
      return ready;
    } catch (error) {
      if (this.#preparedPlayer === player) {
        this.#preparedPlayer = null;
      }
      if (!published || generation === this.#lifecycleGeneration) {
        terminateCandidate();
      }
      if (generation === this.#lifecycleGeneration) {
        this.#adoptPlayerCanvas(player);
      }
      if (generation !== this.#lifecycleGeneration) {
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      throw error;
    }
  }

  #createPlayer() {
    return new ExecutionWorkerClient(this.#canvas, {
      onError: this.#onError,
      onRecoverableError: this.#onRecoverableError,
    });
  }

  #resolveStartupSharedSlotCapacity(sharedSlotCapacity) {
    if (sharedSlotCapacity !== undefined) {
      return validateSharedSlotCapacity(sharedSlotCapacity);
    }
    return this.#preparedPlayer === null
      ? DEFAULT_SHARED_SLOT_CAPACITY
      : this.#sharedSlotCapacity;
  }

  async #switchRetainedCanonical(sceneSpecJson) {
    const generation = this.#lifecycleGeneration;
    const player = this.#player;
    try {
      const ready = await player.switchToRetainedCanonical(sceneSpecJson, {
        loopDurationSeconds: this.#loopDurationSeconds,
      });
      this.#assertLifecycleCurrent(generation);
      this.#mode = AUTHORING_EXECUTION_RETAINED;
      this.#rendererBackend = ready.render.backend;
      this.#resizeCurrentCanvas();
      const state = await player.state();
      this.#assertLifecycleCurrent(generation);
      return {
        type: "result",
        operation: "rebuild_retained_scene",
        incremental: false,
        rebuilt: true,
        mode: this.#mode,
        ready,
        ...state,
      };
    } catch (error) {
      if (generation !== this.#lifecycleGeneration) {
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      this.#adoptPlayerCanvas(player);
      throw error;
    }
  }

  async #rebuildRetainedCanonical(sceneSpecJson) {
    const generation = this.#lifecycleGeneration;
    const player = this.#player;
    try {
      const ready = await player.rebuildRetainedCanonical(sceneSpecJson, {
        loopDurationSeconds: this.#loopDurationSeconds,
      });
      this.#assertLifecycleCurrent(generation);
      this.#mode = AUTHORING_EXECUTION_RETAINED;
      this.#rendererBackend = ready.render.backend;
      this.#resizeCurrentCanvas();
      const state = await player.state();
      this.#assertLifecycleCurrent(generation);
      return {
        type: "result",
        operation: "rebuild_retained_scene",
        incremental: false,
        rebuilt: true,
        mode: this.#mode,
        ready,
        ...state,
      };
    } catch (error) {
      if (generation !== this.#lifecycleGeneration) {
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      this.#adoptPlayerCanvas(player);
      throw error;
    }
  }

  async #switchLegacy(sceneJson, { callbacks, authoringClient }) {
    const generation = this.#lifecycleGeneration;
    const player = this.#player;
    try {
      const ready = await player.switchToLegacy(sceneJson, {
        callbacks,
        authoringClient,
        loopDurationSeconds: this.#loopDurationSeconds,
      });
      this.#assertLifecycleCurrent(generation);
      this.#mode = AUTHORING_EXECUTION_LEGACY;
      this.#rendererBackend = ready.render.backend;
      this.#resizeCurrentCanvas();
      const state = await player.state();
      this.#assertLifecycleCurrent(generation);
      return {
        type: "result",
        operation: "rebuild_legacy_scene",
        incremental: false,
        rebuilt: true,
        mode: this.#mode,
        ready,
        ...state,
      };
    } catch (error) {
      if (generation !== this.#lifecycleGeneration) {
        throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
      }
      this.#adoptPlayerCanvas(player);
      throw error;
    }
  }

  async #runTransition(rebuild) {
    if (this.#transition !== null) {
      await this.#transition;
    }
    const transition = rebuild();
    this.#transition = transition;
    try {
      return await transition;
    } finally {
      if (this.#transition === transition) {
        this.#transition = null;
      }
    }
  }

  async #withStablePlayer(operation) {
    for (;;) {
      if (this.#transition !== null) {
        await this.#transition;
        continue;
      }
      this.#requireStarted();
      const player = this.#player;
      const mode = this.#mode;
      try {
        const result = await operation(player, mode);
        if (this.#player !== player) {
          continue;
        }
        return result;
      } catch (error) {
        const transition = this.#transition;
        if (transition !== null) {
          await transition;
          continue;
        }
        if (this.#player !== null && this.#player !== player) {
          continue;
        }
        throw error;
      }
    }
  }

  #adoptPlayerCanvas(player) {
    if (this.#canvas === player.canvas) {
      return;
    }
    this.#canvas = player.canvas;
    this.#observeCanvas();
  }

  #observeCanvas() {
    this.#resizeObserver?.disconnect();
    if (typeof ResizeObserver !== "function") {
      this.#resizeObserver = null;
      return;
    }
    this.#resizeObserver = new ResizeObserver(() => {
      if (this.#player !== null) {
        this.#resizeCurrentCanvas();
      }
    });
    this.#resizeObserver.observe(this.#canvas);
  }

  #resizeCurrentCanvas() {
    if (this.#player === null) {
      return;
    }
    const scale = window.devicePixelRatio || 1;
    this.#player.resize(this.#canvas.clientWidth, this.#canvas.clientHeight, scale);
  }

  #assertLifecycleCurrent(generation, terminateCandidate = null) {
    if (generation === this.#lifecycleGeneration) {
      return;
    }
    terminateCandidate?.();
    throw new Error(LIFECYCLE_CANCELLED_MESSAGE);
  }

  #requireStarted() {
    if (this.#player === null) {
      throw new Error("AuthoringExecutionClient has not been started");
    }
  }
}

function createIdempotentTerminator(player) {
  let terminated = false;
  return () => {
    if (terminated) {
      return;
    }
    terminated = true;
    player.terminate();
  };
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

function validateSharedSlotCapacity(sharedSlotCapacity) {
  if (!Number.isSafeInteger(sharedSlotCapacity) || sharedSlotCapacity <= 0) {
    throw new TypeError("shared execution slot capacity must be a positive safe integer");
  }
  return sharedSlotCapacity;
}

function validateSemanticExecutionDescriptor(descriptor) {
  if (!descriptor || typeof descriptor !== "object" || Array.isArray(descriptor)) {
    throw new TypeError("semantic execution descriptor must be an object");
  }
  if (typeof descriptor.contextId !== "string" || descriptor.contextId.trim() === "") {
    throw new TypeError("semantic execution context ID must be a non-empty string");
  }
  if (descriptor.callbackSessionId !== null && descriptor.callbackSessionId !== undefined &&
      (!Number.isSafeInteger(descriptor.callbackSessionId) || descriptor.callbackSessionId < 0)) {
    throw new TypeError("semantic callback session ID must be a non-negative safe integer");
  }
  if (descriptor.continuationGeneration !== null &&
      descriptor.continuationGeneration !== undefined &&
      (!Number.isSafeInteger(descriptor.continuationGeneration) ||
       descriptor.continuationGeneration <= 0)) {
    throw new TypeError("semantic continuation generation must be a positive safe integer");
  }
  return {
    contextId: descriptor.contextId,
    callbackSessionId: descriptor.callbackSessionId ?? null,
    continuationGeneration: descriptor.continuationGeneration ?? null,
  };
}

function validateSemanticAuthoringClient(authoringClient) {
  if (!authoringClient || typeof authoringClient.attachSemanticExecution !== "function") {
    throw new TypeError("semantic execution requires a Python authoring client");
  }
}
