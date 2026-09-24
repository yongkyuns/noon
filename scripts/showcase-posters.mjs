// Retained preview assets, not a declaration of scene/playback approval. The capture
// report remains the provenance source; source edits require a fresh real capture.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { assertCaptureTime, assertCompletedCapture, captureSchedule } from "./showcase-capture-checks.mjs";

export function posterEvidence(manifest, report) {
  assert.equal(manifest.publication, "preview");
  assert.equal(report.checkoutRevision, report.servedBuildIdentity?.sourceRevision);
  assert.match(report.checkoutRevision, /^[a-f0-9]{40}$/);
  assert.match(report.servedBuildIdentity.buildId, /^[a-f0-9]{64}$/);
  assert.ok(["webgl", "webgpu"].includes(report.backendRequested));
  assert.deepEqual(report.results.map(result => result.id), manifest.entries.map(entry => entry.id));
  return {
    schema: 1, publication: "captured-preview",
    captureRevision: report.checkoutRevision,
    backend: report.backendRequested,
    runtimeBuildIdentity: report.servedBuildIdentity,
    posters: manifest.entries.map((entry, index) => {
      const result = report.results[index];
      assert.equal(result.outcome, "pass", `${entry.id}: capture failed`);
      assert.deepEqual(result.pageErrors, []);
      assert.equal(result.poster, `${entry.id}.png`);
      const time = captureSchedule(entry).posterTime;
      const sample = result.samples.find(sample => sample.requestedTime === time);
      assert.ok(sample, `${entry.id}: representative sample is missing`);
      const { requestedTime, publishedTime, authoredDuration, sourceCompleted, sourceState, completionProbe } = sample;
      if (entry.interaction) {
        assert.equal(result.interaction?.exactClear, true);
        assert.equal(result.interaction?.restartRestoresBase, true);
        assert.equal(result.posterImage.pngSha256, result.interaction.selectedImage.pngSha256);
      } else assert.equal(result.posterImage.pngSha256, sample.pngSha256);
      return {
        id: entry.id, sourceSha256: result.sourceSha256, thumbnailTime: entry.thumbnail_time,
        image: result.posterImage,
        sample: { requestedTime, publishedTime, authoredDuration, sourceCompleted, sourceState, completionProbe },
        ...(entry.interaction ? { interaction: result.interaction } : {}),
      };
    }),
  };
}

export function assertRetainedPoster(entry, record, source, png) {
  const hash = bytes => createHash("sha256").update(bytes).digest("hex");
  assert.equal(record.id, entry.id);
  assert.equal(hash(source), record.sourceSha256, `${entry.id}: source changed; recapture its poster`);
  assert.equal(record.thumbnailTime, entry.thumbnail_time, `${entry.id}: representative time changed`);
  assert.equal(hash(png), record.image.pngSha256, `${entry.id}: retained image does not match its capture`);
  // Decoding is qualified by the real capture workflow. Here, hash equality pins
  // those exact decoded bytes; header checks also protect gallery aspect/framing.
  assert.ok(png.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])));
  assert.equal(png.subarray(12, 16).toString("ascii"), "IHDR");
  // Element screenshots round CSS bounds outward (the pointer capture is 960 x 541).
  // Preserve the actual pixels rather than cropping that presentation to 960 x 540.
  const width = png.readUInt32BE(16), height = png.readUInt32BE(20);
  assert.equal(width, record.image.width);
  assert.equal(height, record.image.height);
  assert.ok(width >= 320 && height >= 180 && Math.abs(height - width * 9 / 16) <= 1,
    `${entry.id}: poster must preserve 16:9 framing within one CSS raster pixel`);
  if (record.sample.completionProbe) {
    assertCompletedCapture(entry, record.sample, captureSchedule(entry).posterTime);
  } else assertCaptureTime(entry, record.sample, entry.thumbnail_time);
  if (entry.interaction) {
    assert.equal(record.interaction?.exactClear, true);
    assert.equal(record.interaction?.restartRestoresBase, true);
    assert.equal(record.image.pngSha256, record.interaction.selectedImage.pngSha256);
    assert.ok(record.interaction.recipe.includes("click"), `${entry.id}: actual selection recipe is missing`);
  }
}
