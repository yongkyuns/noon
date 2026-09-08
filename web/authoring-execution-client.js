import { ExecutionWorkerClient } from "./execution-worker-client.js";

export const AUTHORING_EXECUTION_SEMANTIC = "semantic";
export const SEMANTIC_PACING_REALTIME = "realtime";
export const SEMANTIC_PACING_EXTERNAL_SAMPLES = "external_samples";

const DEFAULT_LOOP_DURATION_SECONDS = 4;
const DEFAULT_SHARED_SLOT_CAPACITY = 1024 * 1024;
const LIFECYCLE_CANCELLED_MESSAGE =
  "AuthoringExecutionClient was terminated during an asynchronous operation";
const EMPTY_HOST_METRICS = Object.freeze({
  enabled: false,
  missedDeadlines: 0,
  droppedLateResults: 0,
});

// Browser lifecycle owner for shared Python-authored execution sessions.
// Rust owns semantic and execution state; this client owns attachment, canvas
// lifecycle, controls and recovery across the real worker boundary.
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

  async startSemanticExecution(
    descriptor,
    {
      authoringClient,
      loopDurationSeconds = DEFAULT_LOOP_DURATION_SECONDS,
      transportMode = undefined,
      sharedSlotCapacity = undefined,
      initiallyPaused = false,
      pacing = SEMANTIC_PACING_REALTIME,
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
    validateSemanticPacing(pacing);
    if (pacing === SEMANTIC_PACING_EXTERNAL_SAMPLES && semantic.continuationGeneration === null) {
      throw new Error("external sample pacing requires a source-owned semantic continuation");
    }
    this.#loopDurationSeconds = validateLoopDurationSeconds(loopDurationSeconds);
    this.#sharedSlotCapacity = this.#resolveStartupSharedSlotCapacity(sharedSlotCapacity);
    const options = {
      loopDurationSeconds: this.#loopDurationSeconds,
      sharedSlotCapacity: this.#sharedSlotCapacity,
      initiallyPaused,
      pacing,
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
        continuationGeneration: semantic.continuationGeneration,
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
    return this.#withStablePlayer((player) => player.advanceTo(timeSeconds));
  }

  async debugFrame() {
    return this.#withStablePlayer((player) => player.debugFrame());
  }

  // Exact samples are strict by default. stopAtSourceCompletion lets bounded
  // consumers finish at an earlier source endpoint; the response reports its
  // actual time and sourceCompleted without replaying any callback.
  async sampleToAuthoredTime(timeSeconds, options = {}) {
    return this.#withStablePlayer((player) => player.sampleToAuthoredTime(timeSeconds, options));
  }

  async advanceToWithRendererObservation(timeSeconds) {
    return this.#withStablePlayer((player) => player.advanceToWithRendererObservation(timeSeconds));
  }

  async setNativeStateInput(source, value) {
    return this.#withStablePlayer((player) => player.setNativeStateInput(source, value));
  }

  async emitNativeEvent(source) {
    return this.#withStablePlayer((player) => player.emitNativeEvent(source));
  }

  async restartPlayback() {
    return this.#withStablePlayer((player) => player.restartPlayback());
  }

  async restart() {
    if (this.#transition !== null) await this.#transition;
    this.#requireStarted();
    return this.#runTransition(async () => {
      const player = this.#player;
      const mode = this.#mode;
      const generation = this.#lifecycleGeneration;
      // A replacement canvas can queue ResizeObserver delivery while its
      // renderer is still being prepared. Resume observation only when ready.
      this.#resizeObserver?.disconnect();
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
        this.#canvas = player.canvas;
        throw error;
      }
    });
  }

  resize(width, height, devicePixelRatio = 1) {
    if (this.#transition !== null) return;
    this.#requireStarted();
    this.#player.resize(width, height, devicePixelRatio);
  }

  terminate() {
    this.#lifecycleGeneration += 1;
    this.#resizeObserver?.disconnect();
    this.#resizeObserver = null;
    const preparedPlayer = this.#preparedPlayer;
    const activePlayer = this.#player;
    preparedPlayer?.terminate();
    if (preparedPlayer !== null && this.#canvas !== preparedPlayer.canvas) {
      this.#canvas = preparedPlayer.canvas;
    }
    this.#preparedPlayer = null;
    activePlayer?.terminate();
    if (activePlayer !== null && this.#canvas !== activePlayer.canvas) {
      this.#canvas = activePlayer.canvas;
    }
    this.#player = null;
    this.#mode = null;
    this.#rendererBackend = "";
    this.#transportMode = null;
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
      if (this.#player !== null && this.#transition === null) {
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

function validateLoopDurationSeconds(loopDurationSeconds) {
  if (!Number.isFinite(loopDurationSeconds) || loopDurationSeconds <= 0) {
    throw new TypeError("loop duration must be positive and finite");
  }
  return loopDurationSeconds;
}

function validateSemanticPacing(pacing) {
  if (pacing !== SEMANTIC_PACING_REALTIME && pacing !== SEMANTIC_PACING_EXTERNAL_SAMPLES) {
    throw new TypeError(`unsupported semantic execution pacing ${pacing}`);
  }
  return pacing;
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
