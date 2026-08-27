import { ExecutionWorkerClient as LegacyExecutionWorkerClient } from "./legacy-execution-worker-client.js";
import { RetainedExecutionWorkerClient } from "./retained-execution-worker-client.js";

export class ExecutionWorkerClient {
  #canvas;
  #activeClient = null;
  #executionMode = null;
  #sceneJson = null;
  #retainedDocumentJson = null;
  #loopDurationSeconds = 4;
  #transportMode = null;
  #onError;
  #legacyClientFactory;
  #retainedClientFactory;
  #replacementResizeObserver = null;

  constructor(
    canvas,
    {
      onError = null,
      legacyClientFactory = (target, options) =>
        new LegacyExecutionWorkerClient(target, options),
      retainedClientFactory = (target, options) =>
        new RetainedExecutionWorkerClient(target, options),
    } = {},
  ) {
    if (onError !== null && typeof onError !== "function") {
      throw new TypeError("ExecutionWorkerClient onError must be a function");
    }
    if (typeof legacyClientFactory !== "function" || typeof retainedClientFactory !== "function") {
      throw new TypeError("execution worker client factories must be functions");
    }
    this.#canvas = canvas;
    this.#onError = onError;
    this.#legacyClientFactory = legacyClientFactory;
    this.#retainedClientFactory = retainedClientFactory;
  }

  get canvas() {
    return this.#canvas;
  }

  get transportMode() {
    return this.#activeClient?.transportMode ?? this.#transportMode;
  }

  get executionMode() {
    return this.#executionMode;
  }

  async start(sceneJson, options = {}) {
    if (this.#activeClient !== null) {
      throw new Error("ExecutionWorkerClient is already started");
    }
    const client = this.#legacyClientFactory(this.#canvas, {
      onError: (error, owner) => this.#notifyError(error, owner),
    });
    this.#activeClient = client;
    this.#executionMode = "legacy";
    try {
      const ready = await client.start(sceneJson, options);
      this.#sceneJson = sceneJson;
      this.#retainedDocumentJson = null;
      this.#loopDurationSeconds = options.loopDurationSeconds ?? 4;
      this.#transportMode = ready.transportMode ?? options.transportMode ?? client.transportMode;
      return ready;
    } catch (error) {
      client.terminate();
      this.#activeClient = null;
      this.#executionMode = null;
      throw error;
    }
  }

  ready() {
    this.#requireStarted();
    return this.#activeClient.ready();
  }

  replaceScene(sceneJson, options = {}) {
    return this.#routeScene("replaceScene", sceneJson, options);
  }

  reconcileScene(sceneJson, options = {}) {
    return this.#routeScene("reconcileScene", sceneJson, options);
  }

  async setLoopDurationSeconds(loopDurationSeconds) {
    this.#requireStarted();
    const result = await this.#activeClient.setLoopDurationSeconds(loopDurationSeconds);
    this.#loopDurationSeconds = loopDurationSeconds;
    return result;
  }

  applyPatchBatch(patchBatchJson) {
    this.#requireStarted();
    if (this.#executionMode === "retained") {
      throw new Error("patch batches are not supported by mixed retained execution yet");
    }
    return this.#activeClient.applyPatchBatch(patchBatchJson);
  }

  configureHostCallbacks(callbacks, authoringClient = null) {
    this.#requireStarted();
    if (this.#executionMode === "retained") {
      if (callbacks !== null && callbacks !== undefined) {
        throw new Error("live Python callbacks are not supported by mixed retained execution yet");
      }
      return Promise.resolve();
    }
    return this.#activeClient.configureHostCallbacks(callbacks, authoringClient);
  }

  state() {
    this.#requireStarted();
    return this.#activeClient.state();
  }

  async metrics() {
    this.#requireStarted();
    const report = await this.#activeClient.metrics();
    if (this.#executionMode !== "retained") {
      return report;
    }
    return {
      ...report,
      engineMetrics: {
        ...report.engineMetrics,
        host: report.engineMetrics?.host ?? disabledHostMetrics(),
      },
    };
  }

  resize(width, height, devicePixelRatio = 1) {
    this.#requireStarted();
    this.#activeClient.resize(width, height, devicePixelRatio);
  }

  async restart() {
    this.#requireStarted();
    const ready = await this.#activeClient.restart();
    const nextCanvas = this.#activeClient.canvas;
    if (nextCanvas !== undefined && nextCanvas !== this.#canvas) {
      this.#canvas = nextCanvas;
      this.#installReplacementResizeObserver();
    }
    this.#transportMode = ready.transportMode ?? this.#activeClient.transportMode ?? this.#transportMode;
    return ready;
  }

  terminate(options = {}) {
    this.#activeClient?.terminate(options);
    this.#activeClient = null;
    this.#executionMode = null;
    this.#replacementResizeObserver?.disconnect();
    this.#replacementResizeObserver = null;
  }

  async #routeScene(operation, sceneJson, options) {
    this.#requireStarted();
    const {
      callbacks = null,
      authoringClient = null,
      loopDurationSeconds = null,
      retainedDocument: explicitRetainedDocument,
    } = options ?? {};
    const retainedDocument =
      explicitRetainedDocument === undefined
        ? consumeRetainedDocument(authoringClient)
        : validateRetainedDocument(explicitRetainedDocument);

    if (hasRetainedObjects(retainedDocument)) {
      if (callbacks !== null && callbacks !== undefined) {
        throw new Error(
          "retained Typst scenes with live Python callbacks are not supported by the retained runtime yet",
        );
      }
      return this.#switchToRetained(sceneJson, retainedDocument, loopDurationSeconds);
    }

    if (this.#executionMode === "retained") {
      return this.#switchToLegacy(
        sceneJson,
        callbacks,
        authoringClient,
        loopDurationSeconds,
      );
    }

    const result = await this.#activeClient[operation](sceneJson, {
      callbacks,
      authoringClient,
      loopDurationSeconds,
    });
    this.#sceneJson = result.sceneJson ?? sceneJson;
    if (loopDurationSeconds !== null && loopDurationSeconds !== undefined) {
      this.#loopDurationSeconds = loopDurationSeconds;
    }
    return result;
  }

  async #switchToRetained(sceneJson, retainedDocument, loopDurationSeconds) {
    const duration = loopDurationSeconds ?? this.#loopDurationSeconds;
    const transportMode = this.#transportMode ?? this.#activeClient.transportMode;
    this.#terminateActive();
    this.#replaceCanvas();

    const client = this.#retainedClientFactory(this.#canvas, {
      onError: (error, owner) => this.#notifyError(error, owner),
    });
    this.#activeClient = client;
    this.#executionMode = "retained";
    const retainedDocumentJson = JSON.stringify(retainedDocument);
    try {
      const ready = await client.start(sceneJson, retainedDocumentJson, {
        loopDurationSeconds: duration,
        ...(transportMode === null || transportMode === undefined ? {} : { transportMode }),
      });
      this.#sceneJson = sceneJson;
      this.#retainedDocumentJson = retainedDocumentJson;
      this.#loopDurationSeconds = duration;
      this.#transportMode = ready.transportMode ?? client.transportMode ?? transportMode;
      this.#installReplacementResizeObserver();
      const state = await client.state();
      return {
        ...state,
        type: "result",
        operation: "replace_scene",
        incremental: false,
        executionMode: "retained",
        session: ready.session ?? null,
        nextPatchSequence: state.nextPatchSequence ?? "0",
      };
    } catch (error) {
      client.terminate();
      this.#activeClient = null;
      this.#executionMode = null;
      throw error;
    }
  }

  async #switchToLegacy(sceneJson, callbacks, authoringClient, loopDurationSeconds) {
    const duration = loopDurationSeconds ?? this.#loopDurationSeconds;
    const transportMode = this.#transportMode ?? this.#activeClient.transportMode;
    this.#terminateActive();
    this.#replaceCanvas();

    const client = this.#legacyClientFactory(this.#canvas, {
      onError: (error, owner) => this.#notifyError(error, owner),
    });
    this.#activeClient = client;
    this.#executionMode = "legacy";
    try {
      const ready = await client.start(sceneJson, {
        loopDurationSeconds: duration,
        ...(transportMode === null || transportMode === undefined ? {} : { transportMode }),
      });
      await client.configureHostCallbacks(callbacks, authoringClient);
      this.#sceneJson = sceneJson;
      this.#retainedDocumentJson = null;
      this.#loopDurationSeconds = duration;
      this.#transportMode = ready.transportMode ?? client.transportMode ?? transportMode;
      this.#installReplacementResizeObserver();
      const state = await client.state();
      return {
        ...state,
        type: "result",
        operation: "replace_scene",
        incremental: false,
        executionMode: "legacy",
        session: ready.session ?? null,
        nextPatchSequence: state.nextPatchSequence ?? "0",
      };
    } catch (error) {
      client.terminate();
      this.#activeClient = null;
      this.#executionMode = null;
      throw error;
    }
  }

  #terminateActive() {
    this.#activeClient?.terminate();
    this.#activeClient = null;
  }

  #replaceCanvas() {
    const previous = this.#canvas;
    if (!previous || typeof previous.cloneNode !== "function" || typeof previous.replaceWith !== "function") {
      throw new Error("execution canvas cannot be replaced after OffscreenCanvas transfer");
    }
    const replacement = previous.cloneNode(false);
    replacement.width = previous.width;
    replacement.height = previous.height;
    replacement.className = previous.className;
    replacement.id = previous.id;
    previous.replaceWith(replacement);
    this.#canvas = replacement;
    this.#replacementResizeObserver?.disconnect();
    this.#replacementResizeObserver = null;
  }

  #installReplacementResizeObserver() {
    this.#replacementResizeObserver?.disconnect();
    this.#replacementResizeObserver = null;
    if (typeof ResizeObserver !== "function") {
      return;
    }
    const canvas = this.#canvas;
    this.#replacementResizeObserver = new ResizeObserver(() => {
      if (this.#activeClient === null) {
        return;
      }
      try {
        this.#activeClient.resize(
          canvas.clientWidth,
          canvas.clientHeight,
          globalThis.devicePixelRatio || 1,
        );
      } catch (error) {
        this.#notifyError(error, "resize");
      }
    });
    this.#replacementResizeObserver.observe(canvas);
  }

  #notifyError(error, owner) {
    this.#onError?.(error, owner);
  }

  #requireStarted() {
    if (this.#activeClient === null) {
      throw new Error("ExecutionWorkerClient has not been started");
    }
  }
}

function consumeRetainedDocument(authoringClient) {
  if (!authoringClient || typeof authoringClient.consumeRetainedDocument !== "function") {
    return null;
  }
  return validateRetainedDocument(authoringClient.consumeRetainedDocument());
}

function validateRetainedDocument(document) {
  if (document === null || document === undefined) {
    return null;
  }
  if (
    typeof document !== "object" ||
    Array.isArray(document) ||
    !Array.isArray(document.objects)
  ) {
    throw new TypeError("retained authoring handoff must contain an objects array");
  }
  return document;
}

function hasRetainedObjects(document) {
  return document !== null && document.objects.length > 0;
}

function disabledHostMetrics() {
  return {
    enabled: false,
    inFlight: false,
    pendingCommit: false,
    generation: 0,
    nextSequence: 0,
    requests: 0,
    completed: 0,
    committed: 0,
    missedDeadlines: 0,
    droppedLateResults: 0,
    errors: 0,
    lastDurationMs: null,
    maxDurationMs: 0,
    lastFrameTime: null,
  };
}
