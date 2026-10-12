export function createDirectExecutionWakeDriver(renderer, options = {}) {
  const now = options.now ?? (() => performance.now());
  const requestAnimationFrameFn =
    options.requestAnimationFrame ?? ((callback) => globalThis.requestAnimationFrame(callback));
  const cancelAnimationFrameFn =
    options.cancelAnimationFrame ?? ((handle) => globalThis.cancelAnimationFrame(handle));
  const setTimeoutFn =
    options.setTimeout ?? ((callback, delay) => globalThis.setTimeout(callback, delay));
  const clearTimeoutFn =
    options.clearTimeout ?? ((handle) => globalThis.clearTimeout(handle));

  let running = true;
  let animationFrameHandle = null;
  let timerHandle = null;
  let idle = false;
  let scheduledAnimationFrames = 0;
  let scheduledTimers = 0;
  let presentationAttempts = 0;
  let presentedFrames = 0;

  function cancelScheduledWake() {
    if (animationFrameHandle !== null) {
      cancelAnimationFrameFn(animationFrameHandle);
      animationFrameHandle = null;
    }
    if (timerHandle !== null) {
      clearTimeoutFn(timerHandle);
      timerHandle = null;
    }
  }

  function readDirective(wallTimeMs) {
    return JSON.parse(renderer.directWakeDirectiveJson(wallTimeMs));
  }

  function presentPending() {
    presentationAttempts += 1;
    const presented = renderer.render();
    if (presented) {
      presentedFrames += 1;
    }
    return presented;
  }

  function scheduleAnimationFrame() {
    // Preserve a refresh opportunity that is still needed; replacing its handle
    // can move delivery to the next refresh when the browser has captured it.
    if (animationFrameHandle !== null) return;
    cancelScheduledWake();
    idle = false;
    scheduledAnimationFrames += 1;
    animationFrameHandle = requestAnimationFrameFn(onAnimationFrame);
  }

  function scheduleTimer(delayMs) {
    idle = false;
    scheduledTimers += 1;
    timerHandle = setTimeoutFn(onTimer, delayMs);
  }

  function schedule() {
    if (!running) {
      return;
    }
    const wallTimeMs = now();
    let directive = readDirective(wallTimeMs);
    if (directive.presentNow) {
      if (!presentPending()) {
        // Surface acquisition can be transiently unavailable. Keep the session's
        // authoritative invalidation intact and retry on one presentation callback.
        scheduleAnimationFrame();
        return;
      }
      directive = readDirective(now());
    }

    switch (directive.cadence) {
      case "animation-frame":
        scheduleAnimationFrame();
        return;
      case "timer":
        cancelScheduledWake();
        scheduleTimer(directive.delayMs);
        return;
      case "idle":
        cancelScheduledWake();
        idle = true;
        return;
      default:
        throw new Error(`unknown direct execution wake cadence: ${directive.cadence}`);
    }
  }

  function onAnimationFrame(timestamp) {
    animationFrameHandle = null;
    if (!running) {
      return;
    }
    renderer.advanceDirectRealtime(timestamp);
    presentPending();
    schedule();
  }

  function onTimer() {
    timerHandle = null;
    if (!running) {
      return;
    }
    const wallTimeMs = now();
    renderer.advanceDirectRealtime(wallTimeMs);
    presentPending();
    schedule();
  }

  schedule();

  return {
    wake() {
      schedule();
    },
    stop() {
      running = false;
      idle = false;
      cancelScheduledWake();
    },
    stats() {
      return {
        idle,
        scheduledAnimationFrames,
        scheduledTimers,
        presentationAttempts,
        presentedFrames,
      };
    },
  };
}
