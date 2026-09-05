const DEFAULT_DELAY_MS = 180;

function requireFunction(value, label) {
  if (typeof value !== "function") {
    throw new TypeError(`${label} must be a function`);
  }
  return value;
}

function requireExampleId(exampleId) {
  if (typeof exampleId !== "string" || exampleId.trim() === "") {
    throw new TypeError("latest-source runner requires a non-empty example ID");
  }
  return exampleId;
}

function checkedNext(current) {
  if (!Number.isSafeInteger(current) || current < 0 || current >= Number.MAX_SAFE_INTEGER) {
    throw new Error("latest-source runner generation space exhausted");
  }
  return current + 1;
}

/// Coalesces editor-triggered full-source reruns without owning authored or execution state.
///
/// Noon still performs each accepted run through the existing Python authoring -> Semantic Scene
/// -> execution reconciliation path. This helper only ensures that edits arriving while a run is
/// in flight collapse to one rerun of the latest editor source instead of being silently absorbed
/// by the playground's in-flight Run guard.
export class LatestSourceRunner {
  #run;
  #runInFlight;
  #currentExampleId;
  #delayMs;
  #setTimer;
  #clearTimer;
  #timer = null;
  #requestedVersion = 0;
  #completedVersion = 0;
  #requestedExampleId = null;
  #drainPromise = null;
  #disposed = false;

  constructor({
    run,
    runInFlight,
    currentExampleId,
    delayMs = DEFAULT_DELAY_MS,
    setTimer = globalThis.setTimeout.bind(globalThis),
    clearTimer = globalThis.clearTimeout.bind(globalThis),
  }) {
    this.#run = requireFunction(run, "latest-source run");
    this.#runInFlight = requireFunction(runInFlight, "latest-source runInFlight");
    this.#currentExampleId = requireFunction(
      currentExampleId,
      "latest-source currentExampleId",
    );
    if (!Number.isFinite(delayMs) || delayMs < 0) {
      throw new TypeError("latest-source delay must be a non-negative finite number");
    }
    this.#delayMs = delayMs;
    this.#setTimer = requireFunction(setTimer, "latest-source setTimer");
    this.#clearTimer = requireFunction(clearTimer, "latest-source clearTimer");
  }

  get diagnostics() {
    return Object.freeze({
      requestedVersion: this.#requestedVersion,
      completedVersion: this.#completedVersion,
      requestedExampleId: this.#requestedExampleId,
      pending: this.#completedVersion < this.#requestedVersion,
      draining: this.#drainPromise !== null,
      disposed: this.#disposed,
    });
  }

  request(exampleId, { immediate = false } = {}) {
    if (this.#disposed) return null;
    this.#requestedVersion = checkedNext(this.#requestedVersion);
    this.#requestedExampleId = requireExampleId(exampleId);
    this.#cancelTimer();

    if (immediate) {
      return this.#drain();
    }

    this.#timer = this.#setTimer(() => {
      this.#timer = null;
      void this.#drain();
    }, this.#delayMs);
    return null;
  }

  flush() {
    if (this.#disposed) return Promise.resolve();
    this.#cancelTimer();
    return this.#drain();
  }

  dispose() {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#cancelTimer();
  }

  #cancelTimer() {
    if (this.#timer === null) return;
    this.#clearTimer(this.#timer);
    this.#timer = null;
  }

  #drain() {
    if (this.#disposed) return Promise.resolve();
    if (this.#drainPromise !== null) return this.#drainPromise;

    const task = (async () => {
      while (!this.#disposed && this.#completedVersion < this.#requestedVersion) {
        const targetVersion = this.#requestedVersion;
        const targetExampleId = this.#requestedExampleId;

        // Selection changes have their own run path. Do not replay an edit request from the
        // previous example against the newly selected source.
        if (targetExampleId !== this.#currentExampleId()) {
          this.#completedVersion = targetVersion;
          continue;
        }

        // If an explicit/gallery Run is already active, wait for it but do not claim that it
        // contained the edit which arrived afterward. The loop will issue one fresh run next.
        const joinedExistingRun = Boolean(this.#runInFlight());
        await this.#run();
        if (joinedExistingRun) {
          continue;
        }

        this.#completedVersion = targetVersion;
      }
    })();

    this.#drainPromise = task;
    void task.then(
      () => {
        if (this.#drainPromise === task) this.#drainPromise = null;
      },
      () => {
        if (this.#drainPromise === task) this.#drainPromise = null;
      },
    );
    return task;
  }
}

export const LIVE_SOURCE_RERUN_DELAY_MS = DEFAULT_DELAY_MS;
