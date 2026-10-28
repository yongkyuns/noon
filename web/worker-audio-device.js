// Disposable Window output for the authoring worker's finite audio profile.
// Rust validates the immutable resource bundle, computes clip phase/envelopes,
// and owns Web Audio scheduling. This adapter owns only browser lifecycle.
let audioModule;

async function loadAudioModule() {
  audioModule ??= import("./pkg/noon_web.js").then(async module => {
    await module.default();
    return module;
  });
  const pending = audioModule;
  try {
    return await pending;
  } catch (error) {
    // Preserve this failure for its callers, but let a later explicit install
    // retry WASM initialization. Never clear a newer attempt or retry Play.
    if (audioModule === pending) audioModule = undefined;
    throw error;
  }
}

export class WorkerAudioDevice {
  #output = null;
  #context = null;
  #generation = 0;
  #started = false;
  #volume = 1;
  #muted = false;
  #onInterrupted;

  constructor(onInterrupted) {
    this.#onInterrupted = onInterrupted;
  }

  get available() { return this.#output !== null; }
  get ready() { return this.#output?.ready() ?? false; }
  get volume() { return this.#volume; }
  get muted() { return this.#muted; }

  async install(bytes) {
    this.dispose();
    if (bytes === undefined) return;
    if (!(bytes instanceof Uint8Array) || bytes.byteLength === 0) {
      throw new Error("worker audio resources must be a nonempty byte bundle");
    }
    const generation = this.#generation;
    const { WasmWorkerAudioOutput } = await loadAudioModule();
    if (generation !== this.#generation) throw new Error("worker audio preparation was retired");
    const output = new WasmWorkerAudioOutput(bytes);
    try {
      output.setVolume(this.#volume);
      output.setMuted(this.#muted);
      this.#context = output.audioContext();
      this.#context.addEventListener("statechange", this.#stateChanged);
      this.#output = output;
    } catch (error) {
      output.stop();
      output.free();
      throw error;
    }
  }

  // Called synchronously by the gesture handler before any worker round trip.
  unlock() { return this.#output?.unlock() ?? Promise.resolve(); }

  applyAnchor(anchor, session, bracket = null) {
    if (this.#output === null || anchor === undefined) return;
    if (!anchor || anchor.session !== session || typeof anchor.playing !== "boolean") {
      throw new Error("worker audio anchor belongs to a different playback session");
    }
    if (!anchor.playing) {
      this.pause();
      return;
    }
    if (!this.ready) throw new Error("Play with sound to enable audio output");
    // Diagnostic-only: read the same Window clock ONCE at the existing native
    // Start boundary. Neither the strict Rust guard nor its four arguments change.
    const origin = performance.timeOrigin;
    const appliedMs = performance.now();
    const receivedEpochMs = origin + appliedMs;
    const trace = () => bracket === null ? null : ({
      schema: 1, requestId: bracket.requestId, requestType: bracket.type,
      session: bracket.session, lifecycle: bracket.lifecycle,
      sentMs: bracket.sentMs, receivedMs: bracket.receivedMs, appliedMs,
      observableAgeLowerMs: appliedMs - bracket.receivedMs,
      observableAgeUpperMs: appliedMs - bracket.sentMs,
      authoredTime: anchor.time, sampledEpochMs: anchor.sampledEpochMs,
      receivedEpochMs, reconstructedEpochAgeMs: receivedEpochMs - anchor.sampledEpochMs,
    });
    try {
      this.#output.start(session, anchor.time, anchor.sampledEpochMs, receivedEpochMs);
    } catch (error) {
      // Retain the exact rejected call and bracket in the unchanged fixture's
      // captured console-error evidence, without changing the thrown value.
      try { console.error("[Noon #2300 Window bracket]", JSON.stringify(trace()), String(error)); }
      catch { /* Diagnostic collection may not replace the native rejection. */ }
      throw error;
    }
    // The existing unchanged smoke fixture records each successful Start in
    // audioQualification. Attach optional diagnostic data only when that
    // fixture is present; no production scheduling state is retained.
    try {
      const last = globalThis.audioQualification?.anchors?.at(-1);
      if (last && last.session === session && last.time === anchor.time &&
          last.sampledEpochMs === anchor.sampledEpochMs &&
          last.receivedEpochMs === receivedEpochMs) {
        last.windowClockBracket = trace();
      }
    } catch { /* Instrumentation must not affect successful playback. */ }
    this.#started = true;
  }

  pause() {
    this.#started = false;
    this.#output?.pause();
  }

  stop() {
    this.#started = false;
    this.#output?.stop();
  }

  setVolume(volume) {
    if (!Number.isFinite(volume) || volume < 0 || volume > 1) {
      throw new TypeError("audio volume must be between zero and one");
    }
    this.#output?.setVolume(volume);
    this.#volume = volume;
  }

  setMuted(muted) {
    if (typeof muted !== "boolean") throw new TypeError("audio mute must be boolean");
    this.#output?.setMuted(muted);
    this.#muted = muted;
  }

  dispose() {
    this.#generation += 1;
    this.#context?.removeEventListener("statechange", this.#stateChanged);
    this.#context = null;
    this.stop();
    this.#output?.free();
    this.#output = null;
  }

  #stateChanged = () => {
    if (this.#started && !this.ready) {
      this.stop();
      this.#onInterrupted();
    }
  };
}
