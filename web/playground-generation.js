function checkedNext(current, label) {
  if (!Number.isSafeInteger(current) || current < 0 || current >= Number.MAX_SAFE_INTEGER) {
    throw new Error(`${label} space exhausted`);
  }
  return current + 1;
}

function requireExampleId(exampleId) {
  if (typeof exampleId !== "string" || exampleId.trim() === "") {
    throw new TypeError("playground generation requires a non-empty example ID");
  }
  return exampleId;
}

/// Tracks three related but distinct notions of freshness in the public playground:
///
/// 1. selection requests -- source loads may complete out of order;
/// 2. committed selections -- only a loaded, newest request may become active;
/// 3. scene runs -- a run is valid only for the committed selection that started it.
///
/// `commitSelection()` also advances the run generation. That explicitly invalidates
/// authoring work from the previous selection before the new selection becomes visible.
export class PlaygroundGeneration {
  #selectionRequestGeneration = 0;
  #selectionGeneration = 0;
  #runGeneration = 0;
  #staleDrops = 0;
  #lastStale = null;

  beginSelectionRequest(exampleId) {
    const normalizedExampleId = requireExampleId(exampleId);
    const nextGeneration = checkedNext(
      this.#selectionRequestGeneration,
      "playground selection request generation",
    );
    this.#selectionRequestGeneration = nextGeneration;
    return Object.freeze({
      kind: "selection-request",
      requestGeneration: nextGeneration,
      exampleId: normalizedExampleId,
    });
  }

  isSelectionRequestCurrent(token) {
    return (
      token?.kind === "selection-request" &&
      token.requestGeneration === this.#selectionRequestGeneration
    );
  }

  commitSelection(requestToken) {
    if (!this.isSelectionRequestCurrent(requestToken)) {
      return null;
    }
    const nextSelectionGeneration = checkedNext(
      this.#selectionGeneration,
      "playground selection generation",
    );
    // Preflight both counters before publishing either new generation. A failed
    // commit must not partially invalidate the currently visible selection/run.
    const nextRunGeneration = checkedNext(this.#runGeneration, "playground run generation");
    this.#selectionGeneration = nextSelectionGeneration;
    this.#runGeneration = nextRunGeneration;
    return Object.freeze({
      kind: "selection",
      requestGeneration: requestToken.requestGeneration,
      selectionGeneration: nextSelectionGeneration,
      runGeneration: nextRunGeneration,
      exampleId: requestToken.exampleId,
    });
  }

  isSelectionCurrent(token) {
    return (
      token?.kind === "selection" &&
      token.requestGeneration === this.#selectionRequestGeneration &&
      token.selectionGeneration === this.#selectionGeneration
    );
  }

  beginRun(exampleId) {
    const normalizedExampleId = requireExampleId(exampleId);
    const nextGeneration = checkedNext(this.#runGeneration, "playground run generation");
    this.#runGeneration = nextGeneration;
    return Object.freeze({
      kind: "run",
      selectionGeneration: this.#selectionGeneration,
      runGeneration: nextGeneration,
      exampleId: normalizedExampleId,
    });
  }

  isRunCurrent(token, activeExampleId) {
    return (
      token?.kind === "run" &&
      token.selectionGeneration === this.#selectionGeneration &&
      token.runGeneration === this.#runGeneration &&
      token.exampleId === activeExampleId
    );
  }

  recordStale(token, stage) {
    this.#staleDrops = checkedNext(this.#staleDrops, "playground stale-result counter");
    this.#lastStale = Object.freeze({
      kind: token?.kind ?? "unknown",
      exampleId: token?.exampleId ?? null,
      selectionGeneration: token?.selectionGeneration ?? null,
      runGeneration: token?.runGeneration ?? null,
      requestGeneration: token?.requestGeneration ?? null,
      stage: String(stage ?? "unknown"),
    });
    return this.diagnostics;
  }

  get diagnostics() {
    return Object.freeze({
      selectionRequestGeneration: this.#selectionRequestGeneration,
      selectionGeneration: this.#selectionGeneration,
      runGeneration: this.#runGeneration,
      staleDrops: this.#staleDrops,
      lastStale: this.#lastStale,
    });
  }
}
