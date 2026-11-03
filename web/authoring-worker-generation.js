function requireFunction(value, label) {
  if (typeof value !== "function") throw new TypeError(`${label} must be a function`);
  return value;
}

function trustOf(client) {
  return client?.libraryTrust === "pristine" || client?.libraryTrust === "tainted"
    ? client.libraryTrust
    : "unknown";
}

/// Owns the current authoring interpreter and, only when needed, one pristine replacement.
/// Source execution and cancellation remain with the existing playground lifecycle.
export class AuthoringWorkerGeneration {
  #createClient;
  #isCurrentRun;
  #activeClient = null;
  #candidate = null;
  #latestToken = null;
  #disposed = false;

  constructor({ createClient, isCurrentRun }) {
    this.#createClient = requireFunction(createClient, "authoring worker createClient");
    this.#isCurrentRun = requireFunction(isCurrentRun, "authoring worker isCurrentRun");
  }

  get activeClient() {
    return this.#activeClient;
  }

  async clientForRun(token, { requirePristine = true } = {}) {
    if (typeof requirePristine !== "boolean") {
      throw new TypeError("requirePristine must be a boolean");
    }
    if (!this.#isCurrentRun(token)) return null;
    // A permanently navigated-away playground cannot admit another Run.
    // Returning null keeps a late explicit call from surfacing teardown as a
    // Python error; cached documents keep this owner undisposed and reusable.
    if (this.#disposed) return null;
    this.#latestToken = token;

    if (this.#activeClient !== null &&
        this.#activeClient.terminated !== true &&
        (!requirePristine || trustOf(this.#activeClient) === "pristine")) {
      return this.#activeClient;
    }

    const candidate = this.#ensureCandidate();
    try {
      await Promise.race([candidate.readyPromise, candidate.cancelledPromise]);
    } catch (error) {
      if (this.#candidate !== candidate || this.#disposed || !this.#isCurrentRun(token)) return null;
      this.#discardCandidate(candidate);
      throw error;
    }

    if (this.#disposed || !this.#isCurrentRun(token)) {
      if (this.#latestToken === token) this.#discardCandidate(candidate);
      return null;
    }
    if (this.#candidate !== candidate) return null;
    if (requirePristine && trustOf(candidate.client) !== "pristine") {
      this.#discardCandidate(candidate);
      throw new Error("replacement Python worker did not confirm a pristine interpreter");
    }

    const previous = this.#activeClient;
    this.#activeClient = candidate.client;
    this.#candidate = null;
    if (previous !== null && previous !== this.#activeClient) previous.terminate();
    return this.#activeClient;
  }

  retire(client) {
    if (client === null || client !== this.#activeClient) return false;
    this.#activeClient = null;
    client.terminate();
    return true;
  }

  retireCandidate() {
    const candidate = this.#candidate;
    if (candidate === null) return false;
    this.#candidate = null;
    candidate.cancel(new Error("replacement Python worker startup was superseded"));
    candidate.client.terminate();
    return true;
  }

  dispose() {
    if (this.#disposed) return;
    this.#disposed = true;
    const active = this.#activeClient;
    const candidate = this.#candidate;
    this.#activeClient = null;
    this.#candidate = null;
    active?.terminate();
    if (candidate?.client !== active) {
      candidate?.cancel(new Error("authoring worker generation disposed"));
      candidate?.client.terminate();
    }
  }

  #ensureCandidate() {
    if (this.#candidate !== null) return this.#candidate;
    const client = this.#createClient();
    if (client === null || typeof client.ready !== "function" || typeof client.terminate !== "function") {
      throw new TypeError("authoring worker factory must return a readyable client");
    }
    let cancel;
    const cancelledPromise = new Promise((_, reject) => { cancel = reject; });
    // A pending worker bootstrap must not keep a superseded Run alive forever.
    // The existing replacement watchdog calls retireCandidate at its normal
    // retirement point; this promise makes that termination observable here.
    const candidate = { client, readyPromise: null, cancelledPromise, cancel };
    candidate.readyPromise = Promise.resolve().then(() => client.ready());
    this.#candidate = candidate;
    return candidate;
  }

  #discardCandidate(candidate) {
    if (this.#candidate !== candidate) return;
    this.#candidate = null;
    candidate.client.terminate();
  }
}
