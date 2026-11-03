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
    if (this.#disposed) throw new Error("authoring worker generation is disposed");
    if (!this.#isCurrentRun(token)) return null;
    this.#latestToken = token;

    if (this.#activeClient !== null &&
        this.#activeClient.terminated !== true &&
        (!requirePristine || trustOf(this.#activeClient) === "pristine")) {
      return this.#activeClient;
    }

    const candidate = this.#ensureCandidate();
    try {
      await candidate.readyPromise;
    } catch (error) {
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

  dispose() {
    if (this.#disposed) return;
    this.#disposed = true;
    const active = this.#activeClient;
    const candidate = this.#candidate;
    this.#activeClient = null;
    this.#candidate = null;
    active?.terminate();
    if (candidate?.client !== active) candidate?.client.terminate();
  }

  #ensureCandidate() {
    if (this.#candidate !== null) return this.#candidate;
    const client = this.#createClient();
    if (client === null || typeof client.ready !== "function" || typeof client.terminate !== "function") {
      throw new TypeError("authoring worker factory must return a readyable client");
    }
    const candidate = { client, readyPromise: null };
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
