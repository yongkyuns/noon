// Optional browser-host lifecycle coordination, not a scene or scheduler.
// Factories must return fresh, exclusively owned production authoring/execution
// clients. This bounds host operations; it is NOT an untrusted-code sandbox.
export class SemanticPreviewSession {
  #makeAuthoring;
  #makeExecution;
  #authoring = null;
  #execution = null;
  #state = "new";
  #sourceState = "not_started";
  #duration = null;
  #frame = null;
  #buildIdentity = null;
  #lastRequestedTime = 0;
  #samples = 0;
  #busy = false;
  #failure = null;
  #abort;
  #rejectAbort;
  #limits;
  #cleanupErrors = [];

  constructor({ createAuthoringClient, createExecutionClient, timeoutMs = 30_000,
    maxSourceBytes = 1_000_000, maxSamples = 36_001, maxTimeSeconds = 600 }) {
    if (typeof createAuthoringClient !== "function" || typeof createExecutionClient !== "function") {
      throw new TypeError("preview requires fresh authoring and execution client factories");
    }
    for (const [name, value] of Object.entries({ timeoutMs, maxSourceBytes, maxSamples })) {
      if (!Number.isSafeInteger(value) || value <= 0 || value > 2_147_483_647) {
        throw new RangeError(`${name} must be a positive bounded integer`);
      }
    }
    if (!Number.isFinite(maxTimeSeconds) || maxTimeSeconds <= 0) {
      throw new RangeError("maxTimeSeconds must be positive and finite");
    }
    this.#makeAuthoring = createAuthoringClient;
    this.#makeExecution = createExecutionClient;
    this.#limits = { timeoutMs, maxSourceBytes, maxSamples, maxTimeSeconds };
    this.#abort = new Promise((_, reject) => { this.#rejectAbort = reject; });
    // Failure may arrive while no operation is waiting. Still expose it on the
    // next call without creating an unhandled rejection in the browser host.
    this.#abort.catch(() => {});
  }

  get snapshot() {
    return {
      state: this.#state,
      sourceState: this.#sourceState,
      authoredDuration: this.#duration,
      frame: this.#frame === null ? null : { ...this.#frame },
      error: this.#failure?.message ?? null,
      cleanupErrors: [...this.#cleanupErrors],
      ...(this.#buildIdentity === null ? {} : { buildIdentity: this.#buildIdentity }),
      capabilities: { forwardSampling: this.#state === "ready", seek: false, sceneInspection: false },
    };
  }

  async open(source, { loopDurationSeconds = 4, context = {} } = {}) {
    if (this.#state !== "new") throw new Error("preview session opens only once");
    if (typeof source !== "string" || source.trim() === "") {
      throw new TypeError("preview source must be non-empty");
    }
    if (new TextEncoder().encode(source).byteLength > this.#limits.maxSourceBytes) {
      throw new RangeError("preview source exceeds byte limit");
    }
    if (!Number.isFinite(loopDurationSeconds) || loopDurationSeconds <= 0) {
      throw new RangeError("preview loop duration must be positive and finite");
    }
    this.#state = "opening";
    return this.#perform(async () => {
      this.#authoring = this.#makeAuthoring();
      this.#assertActive();
      const buildIdentity = await this.#authoring.ready();
      this.#buildIdentity = buildIdentity ?? null;
      this.#assertActive();
      this.#sourceState = "running";
      let resolveAttached;
      const attached = new Promise((resolve) => { resolveAttached = resolve; });
      let registered = false;
      const run = this.#authoring.run(source, context, {
        onSemanticContinuation: async (registration) => {
          try {
            this.#assertActive();
            if (registered) throw new Error("preview source registered multiple execution contexts");
            registered = true;
            this.#execution = this.#makeExecution({ onError: (error) => this.#fail(error) });
            this.#assertActive();
            // prepare() owns the candidate before awaiting asynchronous startup.
            // terminate() can therefore retire it even if startup never completes.
            await this.#execution.prepare({ transportMode: "transferable" });
            this.#assertActive();
            await this.#execution.startSemanticExecution(registration.semanticExecution, {
              authoringClient: this.#authoring,
              loopDurationSeconds,
              transportMode: "transferable",
              pacing: "external_samples",
            });
            this.#assertActive();
            resolveAttached();
          } catch (error) {
            this.#fail(error);
            throw error;
          }
        },
      });
      Promise.resolve(run).then((result) => {
        if (this.#state === "closed" || this.#state === "failed") return;
        if (!registered) throw new Error("preview source produced no semantic continuation");
        if (!Number.isFinite(result.duration) || result.duration < 0) {
          throw new Error("preview source returned an invalid duration");
        }
        this.#duration = result.duration;
        this.#sourceState = "completed";
      }).catch((error) => this.#fail(error));
      await Promise.race([attached, this.#abort]);
      this.#assertActive();
      await this.#capture(0);
      this.#assertActive();
      this.#state = "ready";
      return this.snapshot;
    });
  }

  async sample(timeSeconds) {
    this.#assertReady();
    if (this.#busy) throw new Error("preview operation already in progress");
    if (typeof timeSeconds !== "number" || !Number.isFinite(timeSeconds) || timeSeconds < 0) {
      throw new RangeError("preview time must be a non-negative finite number");
    }
    if (timeSeconds < this.#lastRequestedTime) {
      throw new RangeError("preview playback cannot move backwards");
    }
    if (timeSeconds > this.#limits.maxTimeSeconds || this.#samples >= this.#limits.maxSamples) {
      throw new RangeError("preview sampling limit exceeded");
    }
    return this.#perform(async () => {
      await this.#capture(timeSeconds);
      return this.snapshot;
    });
  }

  // Read the last coherent sample, not a new scene inspection or a seek.
  // A separate process runner must add source/build/artifact provenance.
  close(reason = "preview closed") {
    if (this.#state === "closed") return this.snapshot;
    if (typeof reason !== "string" || reason.trim() === "") throw new TypeError("close requires a reason");
    const error = this.#failure ?? new Error(reason);
    this.#state = "closed";
    if (this.#sourceState === "running") this.#sourceState = "canceled";
    this.#rejectAbort(error);
    this.#dispose();
    return this.snapshot;
  }

  async #capture(time) {
    const sample = await this.#execution.sampleToAuthoredTime(time);
    this.#assertActive();
    const report = await this.#execution.metrics();
    this.#assertActive();
    const metrics = report?.metrics;
    if (!Number.isFinite(sample?.time) || sample.time < 0 ||
        typeof metrics?.backend !== "string" || metrics.backend.length === 0) {
      throw new Error("preview host returned invalid sample metadata");
    }
    this.#frame = {
      requestedTime: time,
      publishedTime: sample.time,
      rendererBackend: metrics.backend,
      objectCount: metrics.objectCount ?? null,
      drawCalls: metrics.drawCalls ?? null,
    };
    this.#lastRequestedTime = time;
    this.#samples += 1;
  }

  async #perform(operation) {
    if (this.#busy) throw new Error("preview operation already in progress");
    this.#busy = true;
    let timer;
    const deadline = new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error("preview operation timed out")), this.#limits.timeoutMs);
    });
    try {
      return await Promise.race([operation(), this.#abort, deadline]);
    } catch (error) {
      this.#fail(error);
      throw this.#failure ?? error;
    } finally {
      clearTimeout(timer);
      this.#busy = false;
    }
  }

  #assertReady() {
    this.#assertActive();
    if (this.#state !== "ready") throw new Error("preview scene is not ready");
  }

  #assertActive() {
    // A factory can synchronously report an error before returning its client.
    // Retire that just-returned owner as well as owners present at failure time.
    if (this.#state === "failed" || this.#state === "closed") this.#dispose();
    if (this.#state === "failed") throw this.#failure;
    if (this.#state === "closed") throw new Error("preview session is closed");
  }

  #fail(value) {
    if (this.#state === "closed" || this.#state === "failed") return;
    this.#failure = value instanceof Error ? value : new Error(String(value));
    this.#state = "failed";
    if (this.#sourceState === "running") this.#sourceState = "failed";
    this.#rejectAbort(this.#failure);
    this.#dispose();
  }

  #dispose() {
    // Clear ownership before invoking external cleanup. Both clients are retired
    // even if one terminator throws; subsequent close() remains idempotent.
    const clients = [this.#execution, this.#authoring];
    this.#execution = null;
    this.#authoring = null;
    for (const client of clients) {
      try { client?.terminate(); }
      catch (error) { this.#cleanupErrors.push(String(error)); }
    }
  }
}
