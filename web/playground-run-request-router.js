export function createRunRequestRouter({
  currentSource,
  currentRun,
  activeSourceContinuation,
  activeRunRequest,
  isActiveRunCurrent,
  run,
  supersede,
  onQueued,
}) {
  let transitionPromise = null;

  async function request() {
    if (transitionPromise !== null) {
      const { replacement } = await transitionPromise;
      return replacement;
    }
    const priorRun = currentRun();
    const activeRequest = activeRunRequest();
    if (
      priorRun !== null &&
      activeSourceContinuation() === null &&
      isActiveRunCurrent(activeRequest?.token) &&
      activeRequest?.source === currentSource()
    ) {
      return priorRun;
    }
    if (priorRun === null) return run();

    const transition = (async () => {
      const superseded = await supersede();
      if (!superseded) onQueued();
      await priorRun;
      // Publish the replacement without awaiting it here. The transition gate
      // protects only cancellation/retirement and replacement startup; once
      // the new source owns playback, a later explicit Run may supersede it.
      return { replacement: run() };
    })();
    transitionPromise = transition;
    try {
      const { replacement } = await transition;
      return replacement;
    } finally {
      if (transitionPromise === transition) transitionPromise = null;
    }
  }

  return Object.freeze({ request });
}
