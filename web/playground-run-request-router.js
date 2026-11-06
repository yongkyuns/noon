export function createRunRequestRouter({
  currentSource,
  currentSelection,
  currentSelectionGeneration,
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
    const source = currentSource();
    const exampleId = currentSelection();
    const selectionGeneration = currentSelectionGeneration();
    if (pendingTransition !== null) {
      // The newest explicit Run owns the replacement snapshot while all callers
      // share the same bounded cancellation and worker handoff.
      pendingTransition.isCurrent = isCurrent;
      pendingTransition.source = source;
      pendingTransition.exampleId = exampleId;
      pendingTransition.selectionGeneration = selectionGeneration;
      const { replacement } = await pendingTransition.promise;
      return replacement;
    }
    const priorRun = currentRun();
    const activeRequest = activeRunRequest();
    if (
      priorRun !== null &&
      activeSourceContinuation() === null &&
      isActiveRunCurrent(activeRequest?.token) &&
      activeRequest?.source === source &&
      activeRequest?.exampleId === exampleId
    ) {
      return priorRun;
    }
    if (priorRun === null) return run(source, exampleId, selectionGeneration);

    const transition = { isCurrent, source, exampleId, selectionGeneration, promise: null };
    transition.promise = (async () => {
      const superseded = await supersede();
      if (!superseded && transition.isCurrent()) onQueued();
      await priorRun;
      // Publish the replacement without awaiting it here. The transition gate
      // protects only cancellation/retirement and replacement startup; once
      // the new source owns playback, a later explicit Run may supersede it.
      return { replacement: transition.isCurrent()
        ? run(transition.source, transition.exampleId, transition.selectionGeneration)
        : undefined };
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
