import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import { AgentPreviewSessionRegistry } from "../../../scripts/agent-preview-sessions.mjs";
import { FrameArtifactStore } from "../../../scripts/agent-preview-artifacts.mjs";
import { createDockerPreviewSessionFactory } from "./preview-runner.mjs";

const DEFAULT_MAX_FRAMES_PER_SESSION = 32;
const REGISTRY_LIMIT_KEYS = new Set(["maxScopes", "maxSessions", "maxSessionsPerScope"]);

/**
 * Transport-neutral ownership for preview execution plus retained frame evidence.
 *
 * Execution/session authority remains in AgentPreviewSessionRegistry and the
 * supplied preview-session factory. Frame ownership remains in FrameArtifactStore.
 * This layer only keeps the private association between those two capabilities.
 * It does not expose MCP, CLI, scene/runtime, renderer, or seek semantics.
 */
export class AgentPreviewService {
  #registry;
  #artifacts;
  #scopes = new Map();
  #maxFramesPerSession;
  #disposed = false;
  #disposePromise = null;

  constructor({ createSession, isolationConfig, registryLimits = {}, artifactLimits = {},
    maxFramesPerSession = DEFAULT_MAX_FRAMES_PER_SESSION } = {}) {
    if (createSession === undefined) {
      if (!isolationConfig || typeof isolationConfig !== "object") {
        throw new TypeError("preview service requires createSession or Docker isolationConfig");
      }
      createSession = createDockerPreviewSessionFactory(isolationConfig);
    }
    if (typeof createSession !== "function") throw new TypeError("createSession must be a function");
    if (!Number.isSafeInteger(maxFramesPerSession) || maxFramesPerSession <= 0 || maxFramesPerSession > 65_536) {
      throw new RangeError("maxFramesPerSession must be a positive bounded integer");
    }
    if (!plainRecord(registryLimits) || !plainRecord(artifactLimits)) {
      throw new TypeError("preview service limits must be plain records");
    }
    if (Reflect.ownKeys(registryLimits).some((key) => !REGISTRY_LIMIT_KEYS.has(key))) {
      throw new TypeError("unknown preview session registry limit");
    }
    if (Object.hasOwn(artifactLimits, "maxFramesPerScope")) {
      throw new TypeError("configure retained frame count with maxFramesPerSession");
    }
    this.#maxFramesPerSession = maxFramesPerSession;
    this.#registry = new AgentPreviewSessionRegistry({ createSession, ...registryLimits });
    this.#artifacts = new FrameArtifactStore({
      limits: { ...artifactLimits, maxFramesPerScope: maxFramesPerSession },
    });
  }

  openScope() {
    this.#assertLive();
    const capability = this.#registry.openScope();
    this.#scopes.set(capability, { closed: false, sessions: new Map() });
    return capability;
  }

  async open(scopeCapability, source, options = {}) {
    const scope = this.#scope(scopeCapability);
    if (typeof source !== "string") throw new TypeError("preview source must be text");
    const sourceSha256 = createHash("sha256").update(source, "utf8").digest("hex");
    const openOptions = options ?? {};
    const opened = await this.#registry.open(scopeCapability, source, openOptions);
    const { sessionId, snapshot: capture } = opened;
    let artifactScope = null;
    try {
      this.#assertScopeCurrent(scopeCapability, scope);
      throwIfAborted(openOptions.signal);
      artifactScope = this.#artifacts.openScope({
        sessionId,
        sourceSha256,
        build: artifactBuild(capture?.buildIdentity),
      });
      const artifact = artifactScope.putFrame(artifactFrame(capture));
      throwIfAborted(openOptions.signal);
      this.#assertScopeCurrent(scopeCapability, scope);
      scope.sessions.set(sessionId, {
        artifactScope,
        lastArtifact: artifact,
        frameCount: 1,
        lastRequestedTime: artifact.provenance.requestedTime,
        busy: false,
      });
      return Object.freeze({ sessionId, snapshot: publicSnapshot(capture), artifact });
    } catch (error) {
      artifactScope?.close();
      await cleanupPreserving(error,
        () => this.#registry.close(scopeCapability, sessionId, errorMessage(error, "preview publication failed")));
      throw error;
    }
  }

  async sample(scopeCapability, sessionId, timeSeconds, options = {}) {
    const frames = await this.sampleFrames(scopeCapability, sessionId, [timeSeconds], options);
    return frames[0];
  }

  async sampleFrames(scopeCapability, sessionId, times, options = {}) {
    const { scope, entry } = this.#entry(scopeCapability, sessionId);
    if (entry.busy) throw new Error("preview service operation already in progress");
    const sampleOptions = options ?? {};
    const remaining = this.#maxFramesPerSession - entry.frameCount;
    const normalized = sampleTimes(times, entry.lastRequestedTime, remaining);
    entry.busy = true;
    try {
      throwIfAborted(sampleOptions.signal);
      const results = [];
      for (const timeSeconds of normalized) {
        let capture;
        try {
          capture = await this.#registry.sample(scopeCapability, sessionId, timeSeconds, sampleOptions);
          throwIfAborted(sampleOptions.signal);
        } catch (error) {
          await this.#reconcileFailedOperation(scopeCapability, scope, sessionId, entry, error,
            errorMessage(error, "preview sample failed"));
          throw error;
        }

        try {
          const artifact = entry.artifactScope.putFrame(artifactFrame(capture));
          entry.lastArtifact = artifact;
          entry.frameCount += 1;
          entry.lastRequestedTime = artifact.provenance.requestedTime;
          results.push(Object.freeze({ snapshot: publicSnapshot(capture), artifact }));
        } catch (error) {
          await this.#retirePreserving(scopeCapability, scope, sessionId, entry, error,
            errorMessage(error, "preview artifact publication failed"));
          throw error;
        }
      }
      return Object.freeze(results);
    } finally {
      if (scope.sessions.get(sessionId) === entry) entry.busy = false;
    }
  }

  inspect(scopeCapability, sessionId) {
    const { entry } = this.#entry(scopeCapability, sessionId);
    const snapshot = this.#registry.inspect(scopeCapability, sessionId);
    return Object.freeze({
      snapshot: publicSnapshot(snapshot),
      artifact: entry.lastArtifact,
      retainedFrames: entry.frameCount,
      maxRetainedFrames: this.#maxFramesPerSession,
    });
  }

  getArtifact(scopeCapability, sessionId, artifactId) {
    const { entry } = this.#entry(scopeCapability, sessionId);
    return entry.artifactScope.getFrame(artifactId);
  }

  async close(scopeCapability, sessionId, reason = "preview session closed") {
    const { scope, entry } = this.#entry(scopeCapability, sessionId);
    let result;
    let failure = null;
    try {
      result = await this.#registry.close(scopeCapability, sessionId, reason);
    } catch (error) {
      failure = error;
    } finally {
      entry.artifactScope.close();
      scope.sessions.delete(sessionId);
    }
    if (failure) throw failure;
    return publicSnapshot(result);
  }

  async closeScope(scopeCapability, reason = "preview transport disconnected") {
    const scope = this.#scope(scopeCapability);
    scope.closed = true;
    let result;
    let failure = null;
    try {
      result = await this.#registry.closeScope(scopeCapability, reason);
    } catch (error) {
      failure = error;
    } finally {
      for (const entry of scope.sessions.values()) entry.artifactScope.close();
      scope.sessions.clear();
      this.#scopes.delete(scopeCapability);
    }
    if (failure) throw failure;
    return result;
  }

  dispose(reason = "preview service shutdown") {
    if (this.#disposePromise) return this.#disposePromise;
    if (this.#disposed) return Promise.resolve(Object.freeze([]));
    this.#disposed = true;
    this.#disposePromise = (async () => {
      let result;
      let failure = null;
      try {
        result = await this.#registry.dispose(reason);
      } catch (error) {
        failure = error;
      } finally {
        for (const scope of this.#scopes.values()) {
          for (const entry of scope.sessions.values()) entry.artifactScope.close();
          scope.sessions.clear();
          scope.closed = true;
        }
        this.#scopes.clear();
        this.#artifacts.dispose();
      }
      if (failure) throw failure;
      return result;
    })();
    return this.#disposePromise;
  }

  get stats() {
    return Object.freeze({
      scopes: this.#scopes.size,
      sessions: this.#registry.counts.sessions,
      artifacts: this.#artifacts.stats(),
      maxFramesPerSession: this.#maxFramesPerSession,
      disposed: this.#disposed,
    });
  }

  async #reconcileFailedOperation(scopeCapability, scope, sessionId, entry, error, reason) {
    let stale = false;
    let snapshot = null;
    try { snapshot = this.#registry.inspect(scopeCapability, sessionId); }
    catch { stale = true; }
    if (stale) {
      this.#releaseLocal(scope, sessionId, entry);
      return;
    }
    if (snapshot?.state === "closed" || snapshot?.state === "failed") {
      await this.#retirePreserving(scopeCapability, scope, sessionId, entry, error, reason);
    }
  }

  async #retirePreserving(scopeCapability, scope, sessionId, entry, error, reason) {
    await cleanupPreserving(error, () => this.#registry.close(scopeCapability, sessionId, reason));
    this.#releaseLocal(scope, sessionId, entry);
  }

  #releaseLocal(scope, sessionId, entry) {
    entry.artifactScope.close();
    if (scope.sessions.get(sessionId) === entry) scope.sessions.delete(sessionId);
  }

  #entry(scopeCapability, sessionId) {
    const scope = this.#scope(scopeCapability);
    if (typeof sessionId !== "string" || sessionId.length === 0) {
      throw new TypeError("preview session ID must be non-empty");
    }
    const entry = scope.sessions.get(sessionId);
    if (!entry) throw new Error("stale or cross-scope preview session ID");
    return { scope, entry };
  }

  #scope(capability) {
    this.#assertLive();
    const scope = this.#scopes.get(capability);
    if (!scope || scope.closed) throw new Error("stale preview service scope capability");
    return scope;
  }

  #assertScopeCurrent(capability, scope) {
    if (this.#disposed || scope.closed || this.#scopes.get(capability) !== scope) {
      throw new Error("preview service scope closed during operation");
    }
  }

  #assertLive() {
    if (this.#disposed) throw new Error("preview service is disposed");
  }
}

function artifactBuild(identity) {
  if (!identity || typeof identity !== "object" || Array.isArray(identity)) {
    throw new Error("preview capture lacks observed runtime build identity");
  }
  const files = identity.files;
  if (!files || typeof files !== "object" || Array.isArray(files) ||
      typeof files.worker?.sha256 !== "string" || typeof files.wasm?.sha256 !== "string" ||
      typeof identity.buildId !== "string") {
    throw new Error("preview capture has incomplete observed runtime build identity");
  }
  return {
    engineRevision: identity.sourceRevision ?? null,
    wasmSha256: files.wasm.sha256,
    workerSha256: files.worker.sha256,
    buildId: identity.buildId,
  };
}

function artifactFrame(capture) {
  if (!capture || typeof capture !== "object" || !Buffer.isBuffer(capture.imageData)) {
    throw new Error("preview capture lacks owned PNG bytes");
  }
  const frame = capture.frame;
  if (!frame || typeof frame !== "object") throw new Error("preview capture lacks frame metadata");
  return {
    png: capture.imageData,
    requestedTime: frame.requestedTime,
    publishedTime: frame.publishedTime,
    backend: frame.rendererBackend,
    sceneRevision: null,
    frameRevision: null,
  };
}

function publicSnapshot(value) {
  if (!value || typeof value !== "object") return value;
  const { imageData: _imageData, ...snapshot } = value;
  return Object.freeze(snapshot);
}

function sampleTimes(times, lowerBound, maxCount) {
  if (!Array.isArray(times) || times.length === 0) {
    throw new TypeError("sample times must be a non-empty array");
  }
  if (times.length > maxCount) throw new RangeError("preview retained frame limit exceeded");
  let previous = lowerBound;
  return times.map((value) => {
    if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
      throw new RangeError("preview sample time must be finite and nonnegative");
    }
    if (value < previous) throw new RangeError("preview playback cannot move backwards");
    previous = value;
    return value;
  });
}

function throwIfAborted(signal) {
  if (!signal?.aborted) return;
  if (signal.reason instanceof Error) throw signal.reason;
  throw new Error(typeof signal.reason === "string" && signal.reason.trim() ? signal.reason : "preview operation canceled");
}

async function cleanupPreserving(primaryError, cleanup) {
  try { await cleanup(); }
  catch (cleanupError) {
    if (primaryError && typeof primaryError === "object") {
      primaryError.cleanupError = errorMessage(cleanupError, "preview cleanup failed");
    }
  }
}

function errorMessage(error, fallback) {
  return error instanceof Error && error.message ? error.message : String(error ?? fallback);
}

function plainRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value) &&
    [Object.prototype, null].includes(Object.getPrototypeOf(value));
}
