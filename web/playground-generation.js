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
/// 3. scene runs -- editor selection does not invalidate a run; only another Run or
///    explicit lifecycle cancellation advances its generation.
///
/// A selected example is the current draft. The active run remains owned by its own
/// run token until another explicit execution action replaces it.
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
    this.#selectionGeneration = nextSelectionGeneration;
    return Object.freeze({
      kind: "selection",
      requestGeneration: requestToken.requestGeneration,
      selectionGeneration: nextSelectionGeneration,
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

  beginRun(exampleId, selectionGeneration = this.#selectionGeneration) {
    const normalizedExampleId = requireExampleId(exampleId);
    if (!Number.isSafeInteger(selectionGeneration) || selectionGeneration < 0 ||
        selectionGeneration > this.#selectionGeneration) {
      throw new TypeError("playground run requires a committed selection generation");
    }
    const nextGeneration = checkedNext(this.#runGeneration, "playground run generation");
    this.#runGeneration = nextGeneration;
    return Object.freeze({
      kind: "run",
      selectionGeneration,
      runGeneration: nextGeneration,
      exampleId: normalizedExampleId,
    });
  }

  invalidateRun() {
    const nextGeneration = checkedNext(this.#runGeneration, "playground run generation");
    this.#runGeneration = nextGeneration;
    return nextGeneration;
  }

  isRunCurrent(token) {
    return (
      token?.kind === "run" &&
      token.runGeneration === this.#runGeneration
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
