// Platform task delivery only. The callback performs one bounded Rust source
// step and returns whether another task is needed. No scene data or timeline
// scheduling/interpolation crosses this boundary. A task is not a frame receipt.
export function createCooperativePreparationDriver(prepare, options = {}) {
  if (typeof prepare !== "function") throw new TypeError("prepare must be a function");
  const enqueue = options.setTimeout ?? ((callback) => globalThis.setTimeout(callback, 0));
  const cancel = options.clearTimeout ?? ((handle) => globalThis.clearTimeout(handle));
  let active = true, handle = null, turns = 0, failure = null;
  function schedule() {
    if (active && handle === null) handle = enqueue(run);
  }
  function run() {
    handle = null;
    if (!active) return;
    try {
      const more = prepare();
      if (typeof more !== "boolean") throw new TypeError("prepare must synchronously return a boolean");
      turns++;
      options.onProgress?.();
      if (!more) active = false;
      schedule();
    } catch (error) {
      active = false;
      failure = error;
      // Do not retry arbitrary source code or silently resume a failed run.
      options.onError?.(error);
    }
  }
  schedule();
  return {
    stop() {
      active = false;
      if (handle !== null) { cancel(handle); handle = null; }
    },
    stats() { return { active, turns, failed: failure !== null }; },
  };
}
