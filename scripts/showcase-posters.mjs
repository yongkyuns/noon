// Retained preview assets, not a declaration of scene/playback approval. The capture
// report remains the provenance source; source edits require a fresh real capture.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { assertCaptureTime, assertCompletedCapture, captureSchedule } from "./showcase-capture-checks.mjs";

function assertInteractionEvidence(entry, interaction, image) {
  if (entry.playback_capability === "nonreplayable-native-input") {
    assert.equal(interaction?.automaticRestore, true, `${entry.id}: automaticRestore evidence is missing`);
    assert.equal(interaction?.backgroundNoOp, true, `${entry.id}: backgroundNoOp evidence is missing`);
    assert.equal(interaction?.pointerDrag, true, `${entry.id}: pointerDrag evidence is missing`);
    assert.equal(interaction?.changedPixelsOutsideRightSideRoi, 0, `${entry.id}: ROI confinement evidence is missing`);
    assert.equal(interaction?.runRestoresBase, true, `${entry.id}: runRestoresBase evidence is missing`);
  } else if (entry.features.includes("on_click")) {
    assert.equal(interaction?.automaticRestore, true);
    assert.equal(interaction?.backgroundNoOp, true);
    assert.equal(interaction?.restartRestoresBase, true);
  } else if (entry.interaction) {
    assert.equal(interaction?.exactClear, true);
    assert.equal(interaction?.restartRestoresBase, true);
  } else return;
  assert.equal(image.pngSha256, interaction?.selectedImage?.pngSha256);
}

export function posterEvidence(manifest, report) {
  assert.equal(manifest.publication, "curated");
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
      const sourceDeclaredClick = entry.features.includes("on_click");
      const interactive = entry.interaction || sourceDeclaredClick;
      if (interactive) assertInteractionEvidence(entry, result.interaction, result.posterImage);
      else assert.equal(result.posterImage.pngSha256, sample.pngSha256);
      return {
        id: entry.id, captureBuildId: report.servedBuildIdentity.buildId,
        sourceSha256: result.sourceSha256, thumbnailTime: entry.thumbnail_time,
        image: result.posterImage,
        sample: { requestedTime, publishedTime, authoredDuration, sourceCompleted, sourceState, completionProbe },
        ...(interactive ? { interaction: result.interaction } : {}),
      };
    }),
  };
}

export function assertRetainedPoster(entry, record, source, png, captureIdentity) {
  const hash = bytes => createHash("sha256").update(bytes).digest("hex");
  assert.equal(record.id, entry.id);
  assert.match(record.captureBuildId, /^[a-f0-9]{64}$/);
  assert.equal(record.captureBuildId, captureIdentity?.buildId,
    `${entry.id}: poster belongs to a different runtime build`);
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
  if (entry.features.includes("on_click") || entry.interaction) {
    assertInteractionEvidence(entry, record.interaction, record.image);
    assert.ok(record.interaction.recipe.includes("click"), `${entry.id}: actual selection recipe is missing`);
  }
}

/**
 * Separate source-bound proof for a genuine CUDA fidelity-test screenshot.
 * The earlier 24 showcase posters belong to one different renderer build.
 * Never overwrite their aggregate build identity to make this card fit.
 */
export function assertRetainedCudaFidelityPoster(entry, record, source, png) {
  const hash = bytes => createHash("sha256").update(bytes).digest("hex");
  assert.equal(entry.id, "showcase-cuda-transpose");
  assert.equal(entry.path, "python/examples/cuda_transpose_banks.py");
  assert.equal(entry.thumbnail, "thumbnails/showcase/showcase-cuda-transpose.png");
  assert.equal(record.schema, 2);
  assert.equal(record.publication, "rendered-cuda-fidelity-frame");
  assert.equal(record.id, entry.id);
  assert.equal(record.sourcePath, entry.path);
  assert.equal(record.thumbnail, entry.thumbnail);
  assert.match(record.sourceSha256, /^[a-f0-9]{64}$/, "source identity must be a full SHA-256");
  assert.equal(record.sourceSha256, hash(source), "CUDA poster source changed; recapture required");
  assert.equal(record.image.pngSha256, hash(png), "CUDA poster pixels differ from the captured renderer frame");
  assert.ok(png.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])),
    "CUDA poster must be a real PNG");
  assert.equal(png.subarray(12, 16).toString("ascii"), "IHDR");
  const width = png.readUInt32BE(16), height = png.readUInt32BE(20);
  assert.equal(width, record.image.width);
  assert.equal(height, record.image.height);
  assert.equal(width, 1280);
  assert.equal(height, 720);
  assert.equal(record.sample.requestedTime, entry.thumbnail_time);
  assert.equal(record.sample.publishedTime, entry.thumbnail_time);
  assert.equal(record.sample.operation, "bank");
  assert.equal(record.sample.stride, 33);
  assert.equal(record.sample.column, 3);
  const capture = record.capture;
  assert.equal(capture.repository, "yongkyuns/noon");
  assert.equal(capture.origin, "local-browser-qualification");
  assert.match(capture.testedCommit, /^[a-f0-9]{40}$/);
  for (const field of ["reportSha256", "runnerSha256", "probeSha256"]) {
    assert.match(capture[field], /^[a-f0-9]{64}$/, `CUDA capture has no ${field}`);
  }
  assert.ok(typeof capture.browserVersion === "string" && capture.browserVersion.length > 0);
  for (const obsolete of ["workflowRunId", "artifactId", "testedPrHead"]) {
    assert.equal(capture[obsolete], undefined, "local capture must not inherit old hosted CI provenance");
  }
  assert.equal(capture.reportStatus, "passed");
  assert.ok(Number.isFinite(capture.authoredDurationSeconds) &&
    Math.abs(capture.authoredDurationSeconds - entry.duration) <= 1e-9,
    "CUDA poster storyboard duration differs from the completed source");
  assert.equal(capture.positiveCases, 3);
  assert.equal(capture.negativeControls, 6);
  assert.equal(capture.oracle, "independent-cpu-closed-form");
  assert.equal(capture.cudaValidated, false, "do not confuse renderer evidence with CUDA hardware execution");
  assert.equal(capture.rendererBackend, "WebGPU");
  assert.equal(capture.runtimeBuildIdentity.schema, 1);
  assert.equal(capture.runtimeBuildIdentity.sourceRevision, capture.testedCommit);
  const identity = capture.runtimeBuildIdentity;
  const paths = {worker: "./python-worker.js", wasm: "./pkg/noon_web_bg.wasm",
    glue: "./pkg/noon_web.js", verifier: "./runtime-build-verifier.js"};
  assert.deepEqual(Object.keys(identity.files).sort(), Object.keys(paths).sort());
  for (const [key, expectedPath] of Object.entries(paths)) {
    assert.equal(identity.files[key].path, expectedPath);
    assert.match(identity.files[key].sha256, /^[0-9a-f]{64}$/);
  }
  const core = {schema: identity.schema, sourceRevision: identity.sourceRevision, files: identity.files};
  assert.equal(identity.buildId, hash(JSON.stringify(core)), "CUDA runtime identity has an invalid digest");
}
