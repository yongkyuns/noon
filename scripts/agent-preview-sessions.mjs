import { randomUUID } from "node:crypto";

export class AgentPreviewSessionRegistry {
  #createSession;
  #scopes = new Map();
  #sessionCount = 0;
  #disposed = false;
  #limits;

  constructor({ createSession, maxScopes = 32, maxSessions = 64, maxSessionsPerScope = 16 } = {}) {
    if (typeof createSession !== "function") {
      throw new TypeError("session registry requires a preview-session factory");
    }
    for (const [name, value] of Object.entries({ maxScopes, maxSessions, maxSessionsPerScope })) {
      if (!Number.isSafeInteger(value) || value <= 0 || value > 65_536) {
        throw new RangeError(`${name} must be a positive bounded integer`);
      }
    }
    if (maxSessionsPerScope > maxSessions) {
      throw new RangeError("per-scope session limit cannot exceed global session limit");
    }
    this.#createSession = createSession;
    this.#limits = { maxScopes, maxSessions, maxSessionsPerScope };
  }

  openScope() {
    this.#assertLive();
    if (this.#scopes.size >= this.#limits.maxScopes) {
      throw new RangeError("preview scope limit exceeded");
    }
    const capability = Object.freeze(Object.create(null));
    this.#scopes.set(capability, { closed: false, sessions: new Map() });
    return capability;
  }

  async open(scopeCapability, source, options = {}) {
    const scope = this.#scope(scopeCapability);
    const { signal, ...openOptions } = options ?? {};
    this.#assertSignal(signal);
    if (signal?.aborted) throw abortError(signal.reason);
    if (scope.sessions.size >= this.#limits.maxSessionsPerScope || this.#sessionCount >= this.#limits.maxSessions) {
      throw new RangeError("preview session limit exceeded");
    }

    const session = this.#createSession();
    assertSession(session);
    const sessionId = randomUUID();
    const entry = createEntry(session);
    scope.sessions.set(sessionId, entry);
    this.#sessionCount += 1;

    const canceled = cancellation(signal, () => {
      this.#release(scope, sessionId, entry, abortReason(signal));
    });
    try {
      const snapshot = await Promise.race([
        Promise.resolve(session.open(source, openOptions)),
        canceled.promise,
        entry.releasedPromise,
      ]);
      if (scope.closed || entry.released) throw new Error("preview scope closed during open");
      entry.busy = false;
      return Object.freeze({ sessionId, snapshot });
    } catch (error) {
      this.#release(scope, sessionId, entry, errorMessage(error, "preview open failed"));
      throw error;
    } finally {
      canceled.remove();
      entry.busy = false;
    }
  }

  async sample(scopeCapability, sessionId, timeSeconds, { signal } = {}) {
    const { scope, entry } = this.#entry(scopeCapability, sessionId);
    this.#assertSignal(signal);
    if (signal?.aborted) {
      const error = abortError(signal.reason);
      this.#release(scope, sessionId, entry, error.message);
      throw error;
    }
    if (entry.busy) throw new Error("preview session operation already in progress");
    entry.busy = true;
    const canceled = cancellation(signal, () => {
      this.#release(scope, sessionId, entry, abortReason(signal));
    });
    try {
      return await Promise.race([
        Promise.resolve(entry.session.sample(timeSeconds)),
        canceled.promise,
        entry.releasedPromise,
      ]);
    } finally {
      canceled.remove();
      entry.busy = false;
    }
  }

  inspect(scopeCapability, sessionId) {
    const { entry } = this.#entry(scopeCapability, sessionId);
    if (entry.busy) throw new Error("preview session operation already in progress");
    return entry.session.snapshot;
  }

  close(scopeCapability, sessionId, reason = "preview session closed") {
    const { scope, entry } = this.#entry(scopeCapability, sessionId);
    return this.#release(scope, sessionId, entry, requireReason(reason));
  }

  closeScope(scopeCapability, reason = "preview transport disconnected") {
    this.#assertLive();
    const message = requireReason(reason);
    const scope = this.#scopes.get(scopeCapability);
    if (!scope || scope.closed) throw new Error("stale preview scope capability");
    scope.closed = true;
    const cleanup = [];
    for (const [sessionId, entry] of [...scope.sessions]) {
      cleanup.push(this.#release(scope, sessionId, entry, message));
    }
    this.#scopes.delete(scopeCapability);
    return Object.freeze(cleanup);
  }

  dispose(reason = "preview registry shutdown") {
    if (this.#disposed) return;
    const message = requireReason(reason);
    this.#disposed = true;
    for (const [capability, scope] of [...this.#scopes]) {
      scope.closed = true;
      for (const [sessionId, entry] of [...scope.sessions]) {
        this.#release(scope, sessionId, entry, message);
      }
      this.#scopes.delete(capability);
    }
  }

  get counts() {
    return Object.freeze({ scopes: this.#scopes.size, sessions: this.#sessionCount });
  }

  #entry(scopeCapability, sessionId) {
    const scope = this.#scope(scopeCapability);
    if (typeof sessionId !== "string" || sessionId.length === 0) {
      throw new TypeError("preview session ID must be non-empty");
    }
    const entry = scope.sessions.get(sessionId);
    if (!entry || entry.released) throw new Error("stale or cross-scope preview session ID");
    return { scope, entry };
  }

  #scope(capability) {
    this.#assertLive();
    const scope = this.#scopes.get(capability);
    if (!scope || scope.closed) throw new Error("stale preview scope capability");
    return scope;
  }

  #release(scope, sessionId, entry, reason) {
    if (entry.released) return null;
    entry.released = true;
    if (scope.sessions.get(sessionId) === entry) scope.sessions.delete(sessionId);
    this.#sessionCount -= 1;
    entry.rejectReleased(new Error(reason));
    try {
      return entry.session.close(reason);
    } catch (error) {
      return Object.freeze({ cleanupError: errorMessage(error, "preview close failed") });
    }
  }

  #assertLive() {
    if (this.#disposed) throw new Error("preview session registry is disposed");
  }

  #assertSignal(signal) {
    if (signal !== undefined && !(signal instanceof AbortSignal)) {
      throw new TypeError("signal must be an AbortSignal");
    }
  }
}

function createEntry(session) {
  let rejectReleased;
  const releasedPromise = new Promise((_, reject) => { rejectReleased = reject; });
  releasedPromise.catch(() => {});
  return { session, busy: true, released: false, releasedPromise, rejectReleased };
}

function assertSession(session) {
  if (!session || typeof session.open !== "function" || typeof session.sample !== "function" ||
      typeof session.close !== "function" || !("snapshot" in session)) {
    try { session?.close?.("invalid preview session factory result"); } catch {}
    throw new TypeError("preview-session factory returned an invalid session");
  }
}

function cancellation(signal, onAbort) {
  if (!signal) return { promise: new Promise(() => {}), remove: () => {} };
  let rejectCanceled;
  let fired = false;
  const promise = new Promise((_, reject) => { rejectCanceled = reject; });
  promise.catch(() => {});
  const listener = () => {
    if (fired) return;
    fired = true;
    const error = abortError(signal.reason);
    onAbort();
    rejectCanceled(error);
  };
  signal.addEventListener("abort", listener, { once: true });
  if (signal.aborted) listener();
  return { promise, remove: () => signal.removeEventListener("abort", listener) };
}

function abortError(reason) {
  if (reason instanceof Error) return reason;
  return new Error(typeof reason === "string" && reason.trim() ? reason : "preview operation canceled");
}

function abortReason(signal) {
  return errorMessage(abortError(signal?.reason), "preview operation canceled");
}

function errorMessage(error, fallback) {
  return error instanceof Error && error.message ? error.message : String(error ?? fallback);
}

function requireReason(reason) {
  if (typeof reason !== "string" || reason.trim() === "") throw new TypeError("preview close reason must be non-empty");
  return reason;
}
