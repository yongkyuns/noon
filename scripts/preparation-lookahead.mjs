// Offline diagnostic only: no scheduler, publication, or renderer ownership.
// A completed segment must be ready when its interval STARTS, not when it ends.
// Assumes source time zero and constant playback rate. This is an optimistic
// independent-producer estimate, not proof that partial history is publishable.
export function modelLookahead(trace, speed = 1) {
  if (!Number.isFinite(speed) || speed <= 0) {
    return { valid: false, reason: 'invalid playback speed' };
  }
  if (!Array.isArray(trace) || !trace.length) {
    return { valid: false, reason: 'no completed barriers' };
  }
  if (trace.some((x, i) => !x || !Number.isFinite(x.targetSec) || x.targetSec < 0
      || !Number.isFinite(x.completedMs) || x.completedMs < 0
      || (i > 0 && (x.targetSec < trace[i - 1].targetSec
        || x.completedMs < trace[i - 1].completedMs)))) {
    return { valid: false, reason: 'invalid/nonmonotonic recorded frontier' };
  }
  const initialReadyMs = trace[0].completedMs;
  const segments = [];
  let previousEndSec = 0, stallMs = 0, stallEvents = 0;
  for (const x of trace) {
    const neededAtMs = initialReadyMs + (previousEndSec / speed) * 1000 + stallMs;
    if (!Number.isFinite(neededAtMs)) {
      return { valid: false, reason: 'unrepresentable playback deadline' };
    }
    const missingMs = Math.max(0, x.completedMs - neededAtMs);
    if (missingMs > 0) { stallMs += missingMs; stallEvents++; }
    segments.push({ authoredStartSec: previousEndSec, authoredEndSec: x.targetSec,
      producerReadyMs: x.completedMs, neededAtMs, bufferStallMs: missingMs });
    previousEndSec = x.targetSec;
  }
  return { valid: true, modelVersion: 2, speed, initialReadyMs,
    projectedBufferStallMs: stallMs, projectedBufferEvents: stallEvents,
    finalPreparedAtMs: trace.at(-1).completedMs, segments };
}
