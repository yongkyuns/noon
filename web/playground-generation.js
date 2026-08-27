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
    this.#selectionRequestGeneration = checkedNext(
      this.#selectionRequestGeneration,
      "playground selection request generation",
    );
    return Object.freeze({
      kind: "selection-request",
      requestGeneration: this.#selectionRequestGeneration,
      exampleId: requireExampleId(exampleId),
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
    this.#selectionGeneration = checkedNext(
      this.#selectionGeneration,
      "playground selection generation",
    );
    // A newly committed selection supersedes any authoring result that belongs
    // to the previously active example, even before the UI switches examples.
    this.#runGeneration = checkedNext(this.#runGeneration, "playground run generation");
    return Object.freeze({
      kind: "selection",
      requestGeneration: requestToken.requestGeneration,
      selectionGeneration: this.#selectionGeneration,
      runGeneration: this.#runGeneration,
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
    this.#runGeneration = checkedNext(this.#runGeneration, "playground run generation");
    return Object.freeze({
      kind: "run",
      selectionGeneration: this.#selectionGeneration,
      runGeneration: this.#runGeneration,
      exampleId: requireExampleId(exampleId),
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
