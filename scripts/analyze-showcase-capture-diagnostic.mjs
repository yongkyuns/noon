import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";

const root = path.resolve(process.env.EVIDENCE_ROOT ?? "");
const backend = process.env.BACKEND;
const expectedSource = process.env.EXPECTED_SOURCE;
const expectedPatchSha256 = process.env.EXPECTED_PATCH_SHA256;
assert.ok(root && ["webgpu", "webgl"].includes(backend));

const readJson = async (name) => JSON.parse(await readFile(path.join(root, name), "utf8"));
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
const source = (await readFile(path.join(root, "source.txt"), "utf8")).trim();
const patchSha = (await readFile(path.join(root, "harness.sha256"), "utf8"))
  .trim().split(/\s+/)[0];
const runtime = await readJson("runtime-build-identity.json");
const reportBytes = await readFile(path.join(root, "live/report.json"));
const report = JSON.parse(reportBytes);
const originalBytes = await readFile(path.join(root, "original/capture-report.json"));
const original = JSON.parse(originalBytes);
const recordedOriginalSha = (await readFile(path.join(root, "original/capture-report.sha256"), "utf8"))
  .trim().split(/\s+/)[0];

assert.equal(source, expectedSource, "diagnostic source revision mismatch");
assert.equal(patchSha, expectedPatchSha256, "diagnostic harness patch mismatch");
assert.equal(runtime.sourceRevision, expectedSource, "runtime source revision mismatch");
assert.deepEqual(report.servedBuildIdentity, runtime, "served runtime identity mismatch");
assert.equal(report.backendRequested, backend, "report backend mismatch");
assert.equal(report.servedBuildIdentity?.sourceRevision, expectedSource,
  "served runtime source revision mismatch");
assert.equal(original.backendRequested, backend, "original capture backend mismatch");
assert.deepEqual(original.servedBuildIdentity, runtime, "original capture runtime identity mismatch");
assert.equal(digest(originalBytes), recordedOriginalSha, "original capture report hash mismatch");
assert.equal(report.firstPassCaptureReportSha256, recordedOriginalSha,
  "diagnostic did not consume the preserved original report");

const results = [];
for (const id of ["showcase-bloom-emission", "showcase-glass-lens"]) {
  const result = report.results?.find((item) => item.id === id);
  assert.ok(result, `missing diagnostic result ${id}`);
  const candidate = result.unqualifiedTerminalCandidate;
  let candidatePng = null;
  if (candidate?.filename) {
    const bytes = await readFile(path.join(root, "live", candidate.filename));
    candidatePng = { filename: candidate.filename, byteLength: bytes.length, sha256: digest(bytes) };
    assert.equal(bytes.length, candidate.byteLength, `${id} candidate PNG size mismatch`);
  }
  const trace = result.captureDiagnostic ?? null;
  results.push({
    id,
    outcome: result.outcome,
    stage: result.stage,
    error: result.error ?? null,
    firstPassOutcome: result.firstPassOutcome,
    replayOutcome: result.replayOutcome,
    candidatePng,
    unqualifiedTerminalCandidate: candidate ? {
      classification: candidate.classification,
      attempt: candidate.attempt,
      advisoryPixelComparison: candidate.advisoryPixelComparison ?? null,
    } : null,
    captureDiagnostic: trace ? {
      droppedAttempts: trace.droppedAttempts,
      firstHoldObservation: trace.firstHoldObservation,
      firstWrapObservation: trace.firstWrapObservation,
      attempts: (trace.attempts ?? []).slice(0, 4),
    } : null,
  });
}

console.log(JSON.stringify({
  analysis: "preserved diagnostic evidence; candidates remain unqualified",
  diagnosticRunId: 41844658328,
  diagnosticHead: "bb1808cc47b4973dedc7d7683febb22f160f7340",
  backend,
  sourceRevision: source,
  runtimeBuildIdentity: runtime,
  harnessPatchSha256: patchSha,
  originalCaptureReportSha256: recordedOriginalSha,
  diagnosticReportSha256: digest(reportBytes),
  browserVersion: report.browserVersion,
  results,
}, null, 2));
