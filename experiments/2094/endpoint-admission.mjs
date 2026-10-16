// Fail-closed recognition of a completed authored 25 s scene. The renderer may
// advance its *session* for the final coherent publication (and rebuild once),
// without changing the source or host/browser clock. Never accept a stale
// endpoint from another renderer clock or an unsignalled session jump.
export function endpointAdmission({observed,sawRun,sawProgress,epoch,
    progressRebuilds,progressFrames}) {
  const m=observed?.metrics, counts=m?.counters;
  if(!(sawRun&&sawProgress) || !epoch ||
     observed?.runInFlight!==false || observed?.patchState!=='applied' ||
     observed?.backend!=='WebGL2' || observed?.errors?.length ||
     !m?.ready || m.needsPresent || m.bufferedDeltas!==0 ||
     !Number.isFinite(m.time) || Math.abs(m.time-25)>=0.001 ||
     !Number.isSafeInteger(m.frames) || m.frames<=progressFrames ||
     !Number.isSafeInteger(epoch[0]) ||
     !Number.isSafeInteger(m.session) ||
     m.clockOriginMs!==epoch[1] ||
     !Number.isSafeInteger(progressRebuilds) ||
     !Number.isSafeInteger(counts?.rendererRebuilds) ||
     !Number.isSafeInteger(counts?.objectCount) || counts.objectCount<10084 ||
     !Number.isSafeInteger(counts?.drawCalls) || counts.drawCalls<=0) return null;
  const sessionDelta=m.session-epoch[0];
  const rebuildDelta=counts.rendererRebuilds-progressRebuilds;
  if(!((sessionDelta===0&&rebuildDelta===0)||
       (sessionDelta===1&&rebuildDelta===1))) return null;
  return {type:sessionDelta===0?'same-renderer-session':'completion-session-handoff',
    fromSession:epoch[0],toSession:m.session,
    fromRebuilds:progressRebuilds,toRebuilds:counts.rendererRebuilds,
    fromFrames:progressFrames,toFrames:m.frames,
    authoredEndSeconds:m.time,clockOriginMs:m.clockOriginMs};
}
