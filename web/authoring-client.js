export const AUTHORING_CHANNEL = "noon.authoring";
export const AUTHORING_PROTOCOL_VERSION = 7;

export class PythonAuthoringClient {
  #worker;
  #nextRequestId = 0;
  #pending = new Map();
  #continuations = new Map();
  #readyPromise;
  #resolveReady;
  #rejectReady;
  #ready = false;
  #terminated = false;
  #staleResponses = 0;

  constructor(worker = createAuthoringWorker()) {
    this.#worker = worker;
    this.#readyPromise = new Promise((resolve, reject) => {
      this.#resolveReady = resolve;
      this.#rejectReady = reject;
    });
    worker.addEventListener("message", (event) => this.#handleMessage(event.data));
    worker.addEventListener("error", (event) => {
      const details = [
        event.message,
        event.error?.stack,
        event.filename && `${event.filename}:${event.lineno ?? 0}:${event.colno ?? 0}`,
      ].filter(Boolean);
      this.#fail(new Error(details.join("\n") || "Python authoring worker crashed"));
    });
  }

  get terminated() {
    return this.#terminated;
  }

  get diagnostics() {
    return Object.freeze({
      nextRequestId: this.#nextRequestId,
      pendingRequests: this.#pending.size,
      staleResponses: this.#staleResponses,
      terminated: this.#terminated,
    });
  }

  ready() {
    return this.#readyPromise;
  }

  async run(
    source,
    context = {},
    { onSemanticContinuation = null } = {},
  ) {
    if (typeof source !== "string" || source.trim() === "") {
      throw new TypeError("Python authoring source must be a non-empty string");
    }
    if (!isRecord(context)) {
      throw new TypeError("Python authoring context must be an object");
    }
    if (onSemanticContinuation !== null && typeof onSemanticContinuation !== "function") {
      throw new TypeError("onSemanticContinuation must be a function");
    }
    await this.ready();
    const requestId = this.#beginRequest();
    const result = this.#resultFor(requestId, { onSemanticContinuation });
    this.#worker.postMessage(
      envelope("run", {
        requestId,
        source,
        context,
      }),
    );
    return result;
  }

  async attachSemanticExecution(
    contextId,
    controlPort,
    renderPort,
    {
      transportMode,
      sharedSlotCapacity,
      loopDurationSeconds,
      session,
      callbackSessionId = null,
      continuationGeneration = null,
      initiallyPaused = false,
      pacing = "realtime",
      replaceExistingEndpoint = false,
    },
  ) {
    validateSemanticExecutionContextId(contextId);
    if (typeof replaceExistingEndpoint !== "boolean") throw new TypeError("replaceExistingEndpoint must be a boolean");
    if (!(controlPort instanceof MessagePort) || !(renderPort instanceof MessagePort)) {
      throw new TypeError("semantic execution attachment requires control and render MessagePorts");
    }
    if (transportMode !== "shared" && transportMode !== "transferable") {
      throw new TypeError(`unsupported semantic execution transport mode ${transportMode}`);
    }
    if (!Number.isSafeInteger(sharedSlotCapacity) || sharedSlotCapacity <= 0) {
      throw new TypeError("semantic execution shared slot capacity must be a positive safe integer");
    }
    if (!Number.isFinite(loopDurationSeconds) || loopDurationSeconds <= 0) {
      throw new TypeError("semantic execution loop duration must be positive and finite");
    }
    if (!Number.isSafeInteger(session) || session < 0) {
      throw new TypeError("semantic execution session must be a non-negative safe integer");
    }
    if (callbackSessionId !== null &&
        (!Number.isSafeInteger(callbackSessionId) || callbackSessionId < 0)) {
      throw new TypeError("semantic callback session must be a non-negative safe integer");
    }
    if (continuationGeneration !== null &&
        (!Number.isSafeInteger(continuationGeneration) || continuationGeneration <= 0)) {
      throw new TypeError("semantic continuation generation must be a positive safe integer");
    }
    if (typeof initiallyPaused !== "boolean") {
      throw new TypeError("initiallyPaused must be a boolean");
    }
    if (initiallyPaused && continuationGeneration !== null) {
      throw new Error("source-owned semantic continuations cannot start paused");
    }
    if (pacing !== "realtime" && pacing !== "external_samples") {
      throw new TypeError(`unsupported semantic execution pacing ${pacing}`);
    }
    if (pacing === "external_samples" && continuationGeneration === null) {
      throw new Error("external sample pacing requires a source-owned semantic continuation");
    }
    const continuation = continuationGeneration === null
      ? null
      : this.#matchingContinuation(contextId, continuationGeneration);
    await this.ready();
    const requestId = this.#beginRequest();
    const result = this.#resultFor(requestId);
    const payload = {
      requestId,
      contextId,
      controlPort,
      renderPort,
      transportMode,
      sharedSlotCapacity,
      loopDurationSeconds,
      session,
      initiallyPaused,
      pacing,
      replaceExistingEndpoint,
    };
    if (callbackSessionId !== null) payload.callbackSessionId = callbackSessionId;
    if (continuationGeneration !== null) {
      payload.continuationGeneration = continuationGeneration;
      payload.continuationRunRequestId = continuation.runRequestId;
    }
    this.#worker.postMessage(
      envelope("attach_semantic_execution", payload),
      [controlPort, renderPort],
    );
    return result;
  }

  // Until an execution-client transition commits, the context token remains owned
  // by the authoring result, including when attachment or renderer preflight fails.
  // The owner may retry it without reconstructing semantic state, or retire it with
  // this explicit release. A successful transition transfers that duty to the
  // execution client.
  async releaseSemanticExecution(contextId) {
    validateSemanticExecutionContextId(contextId);
    await this.ready();
    const requestId = this.#beginRequest();
    const result = this.#resultFor(requestId);
    this.#worker.postMessage(
      envelope("release_semantic_execution", { requestId, contextId }),
    );
    const response = await result;
    this.#continuations.delete(contextId);
    return response;
  }

  async cancelSemanticContinuation(contextId, continuationGeneration, reason) {
    validateSemanticExecutionContextId(contextId);
    if (!Number.isSafeInteger(continuationGeneration) || continuationGeneration <= 0) {
      throw new TypeError("semantic continuation generation must be a positive safe integer");
    }
    if (typeof reason !== "string" || reason.trim() === "") {
      throw new TypeError("semantic continuation cancellation requires a reason");
    }
    const continuation = this.#matchingContinuation(contextId, continuationGeneration);
    await this.ready();
    const requestId = this.#beginRequest();
    const result = this.#resultFor(requestId);
    this.#worker.postMessage(envelope("cancel_semantic_continuation", {
      requestId,
      contextId,
      continuationGeneration,
      continuationRunRequestId: continuation.runRequestId,
      reason,
    }));
    const response = await result;
    this.#continuations.delete(contextId);
    return response;
  }

  terminate() {
    if (this.#terminated) {
      return;
    }
    this.#terminated = true;
    this.#worker.terminate();
    this.#fail(new Error("Python authoring client was terminated"));
  }

  #beginRequest() {
    if (this.#terminated) {
      throw new Error("Python authoring client has been terminated");
    }
    const requestId = this.#nextRequestId;
    this.#nextRequestId += 1;
    return requestId;
  }

  #resultFor(requestId, metadata = {}) {
    return new Promise((resolve, reject) => {
      this.#pending.set(requestId, { resolve, reject, ...metadata });
    });
  }

  #handleMessage(message) {
    try {
      validateEnvelope(message);
      if (message.type === "ready") {
        if (!this.#ready) {
          this.#ready = true;
          this.#resolveReady();
        }
        return;
      }

      if (message.type === "error") {
        const details = [
          message.message,
          message.diagnostic?.stack,
        ].filter(Boolean);
        const error = new Error(String(details.join("\n") || "Python authoring failed"));
        if (message.requestId === null) {
          this.#fail(error);
          return;
        }
        this.#settle(message.requestId, ({ reject }) => reject(error));
        return;
      }

      if (message.type === "result") {
        const pending = this.#pendingFor(message.requestId);
        if (pending === null) return;
        const result = parseAuthoringResult(message.resultJson);
        const registrationCompletion = pending.continuationRegistrationCompletion;
        if (registrationCompletion === undefined) {
          this.#settle(message.requestId, ({ resolve }) => resolve(result));
        } else {
          void registrationCompletion.then(
            () => {
              if (this.#pending.get(message.requestId) === pending) {
                this.#settle(message.requestId, ({ resolve }) => resolve(result));
              }
            },
            () => {},
          );
        }
        return;
      }

      if (message.type === "semantic_continuation_registered") {
        this.#handleSemanticContinuation(message);
        return;
      }

      if (message.type === "semantic_execution_attached") {
        this.#settle(message.requestId, ({ resolve }) => resolve(message));
        return;
      }

      if (message.type === "semantic_execution_released") {
        this.#settle(message.requestId, ({ resolve }) => resolve(message));
        return;
      }
      if (message.type === "semantic_continuation_cancelled") {
        this.#settle(message.requestId, ({ resolve }) => resolve(message));
        return;
      }

      throw new Error(`Unknown Python authoring message type: ${message.type}`);
    } catch (error) {
      this.#fail(error);
    }
  }

  #handleSemanticContinuation(message) {
    if (!Number.isSafeInteger(message.requestId) || message.requestId < 0 ||
        !Number.isSafeInteger(message.generation) || message.generation <= 0) {
      throw new Error("Python semantic continuation registration is invalid");
    }
    const pending = this.#pending.get(message.requestId);
    if (!pending || typeof pending.onSemanticContinuation !== "function") {
      throw new Error("Python source suspended without a semantic continuation consumer");
    }
    if (pending.continuationRegistered) {
      throw new Error("Python source registered more than one semantic continuation");
    }
    const semanticExecution = validateSemanticExecutionDescriptor(message.semanticExecution);
    if (semanticExecution?.continuationGeneration !== message.generation) {
      throw new Error("Python semantic continuation registration generation does not match");
    }
    const registration = Object.freeze({
      semanticExecution,
      generation: message.generation,
      duration: validateSceneDuration(message.duration),
    });
    if (this.#continuations.has(semanticExecution.contextId)) {
      throw new Error("Python semantic continuation context is already registered");
    }
    pending.continuationRegistered = true;
    this.#continuations.set(semanticExecution.contextId, {
      generation: message.generation,
      runRequestId: message.requestId,
    });
    const registrationCompletion = Promise.resolve()
      .then(() => pending.onSemanticContinuation(registration));
    pending.continuationRegistrationCompletion = registrationCompletion;
    void registrationCompletion.catch((error) => {
      const failure = error instanceof Error ? error : new Error(String(error));
      if (this.#pending.get(message.requestId) === pending) {
        this.#settle(message.requestId, ({ reject }) => reject(failure));
      }
      void this.cancelSemanticContinuation(
        semanticExecution.contextId,
        message.generation,
        failure.message,
      )
        .catch((cancelError) => {
          this.#fail(cancelError instanceof Error ? cancelError : new Error(String(cancelError)));
        });
    });
  }

  #matchingContinuation(contextId, generation) {
    const continuation = this.#continuations.get(contextId);
    if (continuation?.generation !== generation) {
      throw new Error("stale semantic continuation context or generation");
    }
    return continuation;
  }

  #settle(requestId, settle) {
    const pending = this.#pendingFor(requestId);
    if (pending === null) return false;
    settle(pending);
    this.#pending.delete(requestId);
    return true;
  }

  #pendingFor(requestId) {
    if (!Number.isSafeInteger(requestId) || requestId < 0) {
      throw new Error("Python authoring response has an invalid request ID");
    }
    const pending = this.#pending.get(requestId);
    if (!pending) {
      if (requestId < this.#nextRequestId) {
        this.#staleResponses += 1;
        return null;
      }
      throw new Error(`Python authoring response has unissued request ID ${requestId}`);
    }
    return pending;
  }

  #fail(error) {
    if (!this.#terminated) {
      this.#terminated = true;
      this.#worker.terminate();
    }
    if (!this.#ready) {
      this.#rejectReady(error);
    }
    for (const { reject } of this.#pending.values()) {
      reject(error);
    }
    this.#pending.clear();
    this.#continuations.clear();
  }
}

export function parseAuthoringResult(resultJson) {
  if (typeof resultJson !== "string") {
    throw new Error("Python authoring result must be encoded JSON");
  }
  let result;
  try {
    result = JSON.parse(resultJson);
  } catch (error) {
    throw new Error(`Python authoring returned invalid JSON: ${error.message}`);
  }
  if (!isRecord(result)) {
    throw new Error("Python authoring result must be an object");
  }
  if (result.kind !== "semantic_scene") {
    throw new Error(`Unknown Python authoring result kind: ${result.kind}`);
  }
  const semanticExecution = validateSemanticExecutionDescriptor(result.semantic_execution);
  if (semanticExecution === null) throw new Error("Python Scene result requires a semantic execution descriptor");
  return { kind: result.kind, semanticExecution, duration: validateSceneDuration(result.duration) };
}

export function validateSemanticExecutionDescriptor(descriptor) {
  if (descriptor === null || descriptor === undefined) {
    return null;
  }
  if (!isRecord(descriptor)) {
    throw new Error("Python semantic execution descriptor must be an object");
  }
  validateSemanticExecutionContextId(descriptor.context_id);
  const callbackSessionId = descriptor.callback_session_id;
  const continuationGeneration = descriptor.continuation_generation;
  if (
    callbackSessionId !== null &&
    callbackSessionId !== undefined &&
    (!Number.isSafeInteger(callbackSessionId) || callbackSessionId < 0)
  ) {
    throw new TypeError("semantic callback session ID must be a non-negative safe integer");
  }
  if (continuationGeneration !== null && continuationGeneration !== undefined &&
      (!Number.isSafeInteger(continuationGeneration) || continuationGeneration <= 0)) {
    throw new TypeError("semantic continuation generation must be a positive safe integer");
  }
  return Object.freeze({
    contextId: descriptor.context_id,
    ...(callbackSessionId == null ? {} : { callbackSessionId }),
    ...(continuationGeneration == null ? {} : { continuationGeneration }),
  });
}

function validateSemanticExecutionContextId(contextId) {
  if (typeof contextId !== "string" || contextId.trim() === "") {
    throw new TypeError("semantic execution context ID must be a non-empty string");
  }
  return contextId;
}

export function validateSceneDuration(duration) {
  if (!Number.isFinite(duration) || duration < 0) {
    throw new Error("Python Scene duration must be finite and non-negative");
  }
  return duration;
}

function createAuthoringWorker() {
  return new Worker(new URL("./python-worker.js", import.meta.url), {
    name: "noon-python-authoring",
    type: "module",
  });
}

function envelope(type, payload = {}) {
  return {
    channel: AUTHORING_CHANNEL,
    protocolVersion: AUTHORING_PROTOCOL_VERSION,
    type,
    ...payload,
  };
}

function validateEnvelope(message) {
  if (!isRecord(message) || message.channel !== AUTHORING_CHANNEL) {
    throw new Error("Received a message from an unknown authoring channel");
  }
  if (message.protocolVersion !== AUTHORING_PROTOCOL_VERSION) {
    throw new Error(
      `Unsupported authoring protocol version ${message.protocolVersion}`,
    );
  }
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
