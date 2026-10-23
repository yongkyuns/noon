// Platform task delivery only. Rust returns a scheduling disposition, never
// authored time, scene data or a history payload. Waiting has no polling task.
export function createCooperativePreparationDriver(prepare, options = {}) {
  if (typeof prepare !== "function") throw new TypeError("prepare must be a function");
  const enqueue = options.setTimeout ?? ((callback) => globalThis.setTimeout(callback, 0));
  const cancel = options.clearTimeout ?? ((handle) => globalThis.clearTimeout(handle));
  let active = true, handle = null, turns = 0, failure = null;
  let waiting = false, running = false;
  function schedule() {
    if (active && !waiting && !running && handle === null) handle = enqueue(run);
  }
  function run() {
    handle = null;
    if (!active) return;
    running = true;
    try {
      const disposition = prepare();
      if (!["continue", "waiting", "finished"].includes(disposition)) {
        throw new TypeError("prepare must synchronously return continue, waiting or finished");
      }
      turns++;
      if (active) {
        waiting = disposition === "waiting";
        // Complete before notifying observers, so reentrant wake cannot revive it.
        if (disposition === "finished") active = false;
        // A capped no-op is not new output. Waking presentation here would create
        // a producer/presentation feedback loop while both are actually idle.
        if (!waiting) options.onProgress?.();
      }
    } catch (error) {
      active = false;
      waiting = false;
      failure = error;
      // Do not retry arbitrary source code or silently resume a failed run.
      options.onError?.(error);
    } finally {
      running = false;
      schedule();
    }
  }
  schedule();
  return {
    // The existing presentation/control driver reports changed demand. Coalesce
    // notifications into one task; never execute source inline in an input event.
    wake() {
      if (!active || running) return;
      waiting = false;
      schedule();
    },
    stop() {
      active = false;
      waiting = false;
      if (handle !== null) { cancel(handle); handle = null; }
    },
    stats() { return { active, waiting, turns, failed: failure !== null }; },
  };
}
