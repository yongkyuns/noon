import { ExecutionWorkerClient } from "./execution-worker-client.js";
import { RetainedExecutionWorkerClient } from "./retained-execution-worker-client.js";

export const AUTHORING_EXECUTION_LEGACY = "legacy";
export const AUTHORING_EXECUTION_RETAINED = "retained";

const DEFAULT_LOOP_DURATION_SECONDS = 4;
const DEFAULT_SHARED_SLOT_CAPACITY = 1024 * 1024;
const EMPTY_HOST_METRICS = Object.freeze({
  enabled: false,
  missedDeadlines: 0,
  droppedLateResults: 0,
});

/// One browser execution owner for Python authoring output.
///
/// Legacy SceneDocument-only results keep using the mature execution worker pair.
/// Results with retained objects in a `noon.authoring.retained` sidecar switch
/// atomically to the mixed retained engine/render workers. Empty sidecars remain on
/// legacy execution so ordinary geometry scenes do not pay retained rebuild costs.
/// Because the retained worker does not yet support in-place scene reconciliation,
/// retained edits rebuild only at authoring boundaries; playback remains fully
/// retained and performs no per-frame Python/source work.
export class AuthoringExecutionClient {
  #canvas;
  #player = null;
  #mode = null;
  #rendererBackend = "";
  #loopDurationSeconds = DEFAULT_LOOP_DURATION_SECONDS;
  #transportMode = null;
  #sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY;
  #onError;
  #onRecoverableError;
  #resizeObserver = null;
  #transition = null;

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

  async start(
    sceneJson,
    {
      loopDurationSeconds = DEFAULT_LOOP_DURATION_SECONDS,
      transportMode = undefined,
      sharedSlotCapacity = DEFAULT_SHARED_SLOT_CAPACITY,
    } = {},
  ) {
    if (this.#player !== null || this.#transition !== null) {
      throw new Error("AuthoringExecutionClient is already started");
    }
    validateSceneJson(sceneJson);
    this.#loopDurationSeconds = validateLoopDurationSeconds(loopDurationSeconds);
    this.#sharedSlotCapacity = validateSharedSlotCapacity(sharedSlotCapacity);
    const options = {
      loopDurationSeconds: this.#loopDurationSeconds,
      sharedSlotCapacity: this.#sharedSlotCapacity,
    };
    if (transportMode !== undefined) {
      options.transportMode = transportMode;
    }
    const ready = await this.#startLegacy(sceneJson, options);
    this.#transportMode = ready.transportMode;
    return ready;
  }

  async reconcileScene(
    sceneJson,
    {
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
    const duration = validateOptionalLoopDurationSeconds(loopDurationSeconds);
    if (duration !== null) {
      this.#loopDurationSeconds = duration;
    }

    let retainedDocument = null;
    if (retainedDocumentJson !== null && retainedDocumentJson !== undefined) {
      retainedDocument = validateRetainedDocumentJson(retainedDocumentJson);
    }
    if (retainedDocument !== null && retainedDocument.objects.length > 0) {
      if (callbacks !== null && callbacks !== undefined) {
        throw new Error(
          "retained authoring with Python host callbacks is not supported yet; " +
            "split the callback work from retained text instead of silently dropping either",
        );
      }
      return this.#runTransition(() => this.#rebuildRetained(sceneJson, retainedDocumentJson));
    }

    if (this.#mode === AUTHORING_EXECUTION_LEGACY) {
      const result = await this.#player.reconcileScene(sceneJson, {
        callbacks,
        authoringClient,
        loopDurationSeconds: duration,
      });
      return { ...result, mode: this.#mode, rebuilt: false };
    }

    return this.#runTransition(() => this.#rebuildLegacy(sceneJson, { callbacks, authoringClient }));
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

  async restart() {
    return this.#withStablePlayer(async (player, mode) => {
      const ready = await player.restart();
      this.#canvas = player.canvas;
      this.#mode = mode;
      this.#rendererBackend = ready.render.backend;
      this.#transportMode = ready.transportMode;
      this.#observeCanvas();
      this.#resizeCurrentCanvas();
      return { ...ready, mode };
    });
  }

  async applyPatchBatch(patchBatchJson) {
    return this.#withStablePlayer((player, mode) => {
      if (mode !== AUTHORING_EXECUTION_LEGACY) {
        throw new Error("patch batches are not supported by mixed retained execution yet");
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
    this.#resizeObserver?.disconnect();
    this.#resizeObserver = null;
    this.#player?.terminate();
    this.#player = null;
    this.#mode = null;
    this.#rendererBackend = "";
  }

  async #startLegacy(sceneJson, options) {
    const player = new ExecutionWorkerClient(this.#canvas, {
      onError: this.#onError,
      onRecoverableError: this.#onRecoverableError,
    });
    try {
      const ready = await player.start(sceneJson, options);
      this.#player = player;
      this.#mode = AUTHORING_EXECUTION_LEGACY;
      this.#rendererBackend = ready.render.backend;
      this.#resizeCurrentCanvas();
      return ready;
    } catch (error) {
      player.terminate();
      throw error;
    }
  }

  async #rebuildRetained(sceneJson, retainedDocumentJson) {
    const canvas = this.#replaceTransferredCanvas();
    const player = new RetainedExecutionWorkerClient(canvas, { onError: this.#onError });
    try {
      const ready = await player.start(sceneJson, retainedDocumentJson, {
        loopDurationSeconds: this.#loopDurationSeconds,
        transportMode: this.#transportMode,
        sharedSlotCapacity: this.#sharedSlotCapacity,
      });
      this.#player = player;
      this.#mode = AUTHORING_EXECUTION_RETAINED;
      this.#rendererBackend = ready.render.backend;
      this.#resizeCurrentCanvas();
      const state = await player.state();
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
      player.terminate();
      throw error;
    }
  }

  async #rebuildLegacy(sceneJson, { callbacks, authoringClient }) {
    const canvas = this.#replaceTransferredCanvas();
    const player = new ExecutionWorkerClient(canvas, {
      onError: this.#onError,
      onRecoverableError: this.#onRecoverableError,
    });
    try {
      const ready = await player.start(sceneJson, {
        loopDurationSeconds: this.#loopDurationSeconds,
        transportMode: this.#transportMode,
        sharedSlotCapacity: this.#sharedSlotCapacity,
      });
      if (callbacks !== null && callbacks !== undefined) {
        await player.configureHostCallbacks(callbacks, authoringClient);
      }
      this.#player = player;
      this.#mode = AUTHORING_EXECUTION_LEGACY;
      this.#rendererBackend = ready.render.backend;
      this.#resizeCurrentCanvas();
      const state = await player.state();
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
      player.terminate();
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

  #replaceTransferredCanvas() {
    this.#player?.terminate();
    const previous = this.#canvas;
    const replacement = previous.cloneNode(false);
    replacement.width = previous.width;
    replacement.height = previous.height;
    replacement.className = previous.className;
    replacement.id = previous.id;
    for (const attribute of previous.getAttributeNames()) {
      if (attribute !== "width" && attribute !== "height" && attribute !== "class" && attribute !== "id") {
        replacement.setAttribute(attribute, previous.getAttribute(attribute));
      }
    }
    previous.replaceWith(replacement);
    this.#canvas = replacement;
    this.#player = null;
    this.#mode = null;
    this.#rendererBackend = "";
    this.#observeCanvas();
    return replacement;
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

  #requireStarted() {
    if (this.#player === null) {
      throw new Error("AuthoringExecutionClient has not been started");
    }
  }
}

function validateSceneJson(sceneJson) {
  if (typeof sceneJson !== "string" || sceneJson.trim() === "") {
    throw new TypeError("scene must be non-empty JSON text");
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
  if (document?.channel !== "noon.authoring.retained") {
    throw new TypeError("retained document must use noon.authoring.retained");
  }
  if (!Array.isArray(document.objects)) {
    throw new TypeError("retained document objects must be an array");
  }
  return document;
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
