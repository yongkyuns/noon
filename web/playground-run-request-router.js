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
  let pendingTransition = null;

  async function request(isCurrent = () => true) {
    if (!isCurrent()) return;
    if (pendingTransition !== null) {
      // A newer admitted Run can join cancellation, but an edit that has not
      // reached its debounce deadline must invalidate the earlier request.
      pendingTransition.isCurrent = isCurrent;
      const { replacement } = await pendingTransition.promise;
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

    const transition = { isCurrent, promise: null };
    transition.promise = (async () => {
      const superseded = await supersede();
      if (!superseded && transition.isCurrent()) onQueued();
      await priorRun;
      // Publish the replacement without awaiting it here. The transition gate
      // protects only cancellation/retirement and replacement startup; once
      // the new source owns playback, a later explicit Run may supersede it.
      return { replacement: transition.isCurrent() ? run() : undefined };
    })();
    pendingTransition = transition;
    try {
      const { replacement } = await transition.promise;
      return replacement;
    } finally {
      if (pendingTransition === transition) pendingTransition = null;
    }
  }

  return Object.freeze({ request });
}
