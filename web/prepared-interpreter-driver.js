// Interpreter suspension only. The host completes/adopts segments in Rust and
// asks Rust for demand; this module owns neither authored time nor history.
// A driver belongs to one source invocation and is deliberately not restartable.
export function createPreparedInterpreterDriver(options) {
  const { prepareSegment, needsPreparation, finishSource } = options;
  for (const callback of [prepareSegment, needsPreparation, finishSource]) {
    if (typeof callback !== "function") throw new TypeError("prepared interpreter callbacks are required");
  }
  const enqueue = options.setTimeout ?? (callback => globalThis.setTimeout(callback, 0));
  const cancelTask = options.clearTimeout ?? (handle => globalThis.clearTimeout(handle));
  const abort = new AbortController();
  let phase = "new", started = false, failed = false, failure;
  let pending = null, handle = null, working = false;

  function stop(error, state = "failed") {
    if (failed || phase === "finished") return;
    failed = true;
    failure = error;
    phase = state;
    if (handle !== null) { cancelTask(handle); handle = null; }
    const barrier = pending;
    pending = null;
    // Latch retirement before notifying either the host or the interpreter.
    abort.abort(error);
    barrier?.reject(error);
  }

  function demand() {
    const needed = needsPreparation();
    if (typeof needed !== "boolean") throw new TypeError("preparation demand must synchronously return a boolean");
    return needed;
  }

  function schedule() {
    if (failed || pending === null || working || handle !== null) return;
    try { handle = enqueue(turn); } catch (error) { stop(error); }
  }

  async function turn() {
    handle = null;
    if (failed || pending === null || working) return;
    const barrier = pending;
    working = true;
    try {
      if (!barrier.prepared) {
        phase = "preparing";
        // Includes initial asynchronous canvas creation when necessary. The host
        // must observe the signal before publishing after an asynchronous wait.
        const event = await prepareSegment(barrier.context, abort.signal);
        if (failed || pending !== barrier) return;
        // Observed execution services this same interpreter suspension through
        // its existing callback/completion protocol. Do not manufacture complete.
        if (event !== undefined) {
          if (typeof event !== "string") throw new TypeError("observed continuation event must be a string");
          pending = null;
          phase = "running";
          barrier.resolve(event);
          return;
        }
        barrier.prepared = true;
      }
      const needed = demand();
      if (failed || pending !== barrier) return;
      if (!needed) {
        phase = "waiting";
        return;
      }
      pending = null;
      phase = "running";
      // Reuse the existing continuation control event, not a scene/wire payload.
      barrier.resolve('{"kind":"complete"}');
    } catch (error) {
      stop(error);
    } finally {
      working = false;
    }
  }

  return {
    // execute owns ordinary Python module/construct execution and awaits the
    // existing canonical barrier. Neither source nor completion is retried.
    async run(execute) {
      if (started || failed) throw new Error("prepared interpreter driver is single-use");
      if (typeof execute !== "function") throw new TypeError("execute must be a function");
      started = true;
      phase = "running";
      try {
        const result = await execute();
        if (failed) throw failure;
        if (pending !== null) throw new Error("source returned without awaiting its prepared segment");
        phase = "finishing";
        await finishSource(result, abort.signal);
        if (failed) throw failure;
        phase = "finished";
        return result;
      } catch (error) {
        stop(error);
        // Source may catch a failed barrier; it cannot turn the run successful.
        throw failure;
      }
    },
    awaitSegment(context) {
      if (!started || failed || phase === "finishing" || phase === "finished") {
        return Promise.reject(failed ? failure : new Error("no active prepared source invocation"));
      }
      if (pending !== null) {
        const error = new Error("prepared source already has a suspended segment");
        stop(error);
        return Promise.reject(error);
      }
      const result = new Promise((resolve, reject) => {
        pending = { context, resolve, reject, prepared: false };
      });
      schedule();
      return result;
    },
    // Playback/navigation notifications only, not a polling loop. Even a wake
    // from a DOM input handler never executes a continuation synchronously.
    wake() {
      if (failed || pending === null || working) return;
      try {
        if (!pending.prepared || demand()) schedule();
      } catch (error) { stop(error); }
    },
    cancel(reason = new DOMException("prepared source cancelled", "AbortError")) {
      stop(reason, "cancelled");
    },
    state() { return phase; },
  };
}
