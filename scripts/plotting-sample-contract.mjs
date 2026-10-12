// External plotting-test policy only. Engine acknowledgement and renderer
// presentation are different clocks; never replace either with requested time.
const TIME_TOLERANCE = 1e-6;

export function assertSampleReceipt(receipt, requestedTime) {
  if (!Number.isFinite(requestedTime) || requestedTime < 0 ||
      !Number.isFinite(receipt?.time) ||
      Math.abs(receipt.time - requestedTime) >= TIME_TOLERANCE) {
    throw new Error(`wrong authored-time acknowledgement: requested ${requestedTime}, received ${receipt?.time}`);
  }
}

export function pythonPresentedTime(requestedTime, liveCoordinates = false) {
  if (!Number.isFinite(requestedTime) || requestedTime < 0) {
    throw new Error("plot sample time must be finite and non-negative");
  }
  // These are the two quiet waits in live_coordinate_plotting.py, not a
  // general permission to accept a stale frame for an animated sample.
  if (liveCoordinates && requestedTime < 0.25) return 0;
  if (liveCoordinates && requestedTime > 1.25 && requestedTime <= 1.5) return 1.25;
  return requestedTime;
}

export function isPresentedSample(metrics, expectedTime) {
  return metrics?.ready === true && metrics.retained === true &&
    Number.isSafeInteger(metrics.presentedFrames) && metrics.presentedFrames > 0 &&
    Number.isFinite(metrics.time) && Number.isFinite(expectedTime) &&
    Math.abs(metrics.time - expectedTime) < TIME_TOLERANCE;
}
