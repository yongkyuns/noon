// First-pass evidence for historical replay. These checks neither drive source
// execution nor permit a replay to stand in for its independent pixel oracle.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { assertCaptureTime, assertCompletedCapture, captureSchedule } from "./showcase-capture-checks.mjs";

const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const SHA256 = /^[a-f0-9]{64}$/;

export function replayOracle(entry, report, expected) {
  assert.ok(report && typeof report === "object", "first-pass capture report is missing");
  assert.match(expected.buildIdentity?.sourceRevision ?? "", /^[a-f0-9]{40}$/);
  assert.match(expected.buildIdentity?.buildId ?? "", SHA256);
  assert.deepEqual(report.servedBuildIdentity, expected.buildIdentity, "first-pass runtime build differs from replay");
  assert.equal(report.checkoutRevision, expected.buildIdentity.sourceRevision, "first-pass checkout differs from replay");
  assert.equal(report.browserVersion, expected.browserVersion, "first-pass browser differs from replay");
  assert.ok(["webgl", "webgpu"].includes(expected.backendRequested));
  assert.equal(report.backendRequested, expected.backendRequested, "first-pass backend differs from replay");
  assert.equal(report.samplingHz, 30, "first-pass sampling schedule changed");
  assert.deepEqual(report.viewport, { width: 960, height: 540 }, "first-pass viewport changed");
  assert.match(expected.sourceSha256, SHA256);
  const matches = report.results.filter(result => result.id === entry.id);
  assert.equal(matches.length, 1, `${entry.id}: require one independent first pass`);
  const first = matches[0];
  assert.equal(first.outcome, "pass", `${entry.id}: first-pass capture failed`);
  assert.deepEqual(first.pageErrors, [], `${entry.id}: first-pass browser errors`);
  assert.equal(first.sourceSha256, expected.sourceSha256, `${entry.id}: first-pass source differs from replay`);
  const { wanted, completionTime } = captureSchedule(entry);
  assert.deepEqual(first.samples.map(sample => sample.requestedTime), wanted,
    `${entry.id}: every storyboard checkpoint, in order, must have first-pass evidence`);
  const backend = expected.backendRequested === "webgl" ? "WebGL2" : "WebGPU";
  return first.samples.map(sample => {
    const completionProbe = sample.requestedTime === completionTime;
    assert.equal(sample.completionProbe, completionProbe, `${entry.id}: mislabeled completion probe`);
    assert.equal(sample.error, null, `${entry.id}: first-pass sample failed`);
    assert.equal(sample.presented, true, `${entry.id}: first-pass pixels were not presented`);
    assert.equal(sample.rendererBackend, backend, `${entry.id}: first-pass sample used another backend`);
    if (completionProbe) assertCompletedCapture(entry, sample, completionTime);
    else assertCaptureTime(entry, sample, sample.requestedTime);
    assert.equal(sample.width, report.viewport.width);
    assert.equal(sample.height, report.viewport.height);
    assert.match(sample.pngSha256, SHA256);
    assert.match(sample.pixelSha256, SHA256);
    assert.equal(sample.filename, `${entry.id}-${String(sample.requestedTime).replace(".", "_")}.png`,
      `${entry.id}: first-pass image path differs from its sample`);
    assert.ok(Number.isSafeInteger(sample.objectCount) && sample.objectCount > 0);
    if (entry.performance && sample.requestedTime >= 3.1) {
      assert.ok(sample.objectCount >= 600, "dense first-pass sample lost its geometry workload");
    }
    return { ...sample,
      // The probe bound is not an authored frame. Revisit the actual endpoint.
      replayTime: completionProbe ? sample.authoredDuration : sample.requestedTime };
  });
}

export function assertOracleImage(sample, pngBytes, image) {
  assert.equal(hash(pngBytes), sample.pngSha256, "first-pass PNG bytes changed");
  assert.equal(image.width, sample.width, "first-pass image width changed");
  assert.equal(image.height, sample.height, "first-pass image height changed");
  assert.equal(image.data.length, image.width * image.height * 4, "first-pass image is not decoded RGBA");
  assert.equal(hash(image.data), sample.pixelSha256, "first-pass decoded pixels changed");
}

export function assertReplaySample(entry, sample, requestedTime, metrics) {
  assert.equal(requestedTime, sample.replayTime, "replay changed its requested checkpoint");
  assert.equal(metrics.backend, sample.rendererBackend, "replay sample used another backend");
  assert.ok(Number.isFinite(metrics.time) && metrics.time >= 0, "replay published an invalid time");
  // A seek must publish the requested checkpoint, including in a quiet hold.
  // The independent forward capture may reuse pixels only under its existing
  // declared-still-interval policy; it never relabels their published timestamp.
  const roundoff = 8 * Number.EPSILON * Math.max(1, entry.duration);
  assert.ok(Math.abs(metrics.time - requestedTime) <= roundoff, "replay published stale/future pixels");
  assert.ok(Number.isSafeInteger(metrics.objectCount) && metrics.objectCount > 0);
  if (entry.performance && requestedTime >= 3.1) {
    assert.ok(metrics.objectCount >= 600, "dense replay sample lost its geometry workload");
  }
}
