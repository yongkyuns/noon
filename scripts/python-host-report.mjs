// Diagnostic/export boundary only. Keep u64 runtime counters lossless without
// changing the typed observations used by the conformance assertions.
import assert from "node:assert/strict";

export function stringifyEvidence(value) {
  return JSON.stringify(value, (_key, item) =>
    typeof item === "bigint" ? item.toString() : item, 2);
}

export function assertSingleReport(output, name, result) {
  assert.equal(output.length, 1,
    `${name}: expected exactly one Python report; ${stringifyEvidence(result)}`);
}

export function assertRenderedOutput(metrics, requestedBackend) {
  const name = { webgpu: "WebGPU", webgl: "WebGL2" }[requestedBackend];
  assert.ok(name, `unknown renderer selection: ${requestedBackend}`);
  assert.ok(metrics.presentedFrames > 0 && metrics.drawCalls > 0, "no actual rendered output");
  assert.equal(metrics.backend, name, "renderer did not honor requested backend");
}

// A quiet hold can finish without a new renderer publication. The sample's
// clock is authoritative; metrics.time describes the last actual redraw.
export function assertCompletedSample(samples, duration) {
  const last = samples.at(-1);
  assert.ok(last && last.sourceCompleted === true, "missing source-completion sample receipt");
  assert.ok(Number.isFinite(duration) && Number.isFinite(last.time)
    && Math.abs(last.time - duration) <= 2e-5,
    "sample receipt did not reach final authored time");
}
