import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const execFileAsync = promisify(execFile);
const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../../..");
const cliPath = path.join(repoRoot, "tools/noon-mcp/bin/noon-preview.mjs");
const feedbackCliPath = path.join(repoRoot, "tools/noon-mcp/bin/noon-feedback.mjs");
const sourcePath = path.join(repoRoot, "web/python/examples/manim_parity_square_to_circle.py");
const runtimeConfig = process.env.NOON_PREVIEW_RUNTIME_CONFIG;
if (!runtimeConfig) throw new Error("NOON_PREVIEW_RUNTIME_CONFIG is required for Docker CLI smoke");

const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const sourceBytes = await readFile(sourcePath);
const sourceSha256 = hash(sourceBytes);
const runtimeIdentity = JSON.parse(await readFile(path.join(repoRoot, "web/runtime-build-identity.json"), "utf8"));
const expectedBuild = Object.freeze({
  engineRevision: runtimeIdentity.sourceRevision ?? null,
  wasmSha256: runtimeIdentity.files?.wasm?.sha256,
  workerSha256: runtimeIdentity.files?.worker?.sha256,
  buildId: runtimeIdentity.buildId,
});
for (const [name, value] of Object.entries(expectedBuild)) {
  if (name === "engineRevision" && value === null) continue;
  assert.match(value, name === "engineRevision" ? /^[0-9a-f]{40}$/ : /^[0-9a-f]{64}$/,
    `${name} must come from the generated runtime identity`);
}

const root = await mkdtemp(path.join(os.tmpdir(), "noon-preview-cli-real-"));
const outputDir = path.join(root, "frames");
try {
  const { stdout } = await execFileAsync(process.execPath, [
    cliPath,
    "--source", sourcePath,
    "--output", outputDir,
    "--loop-duration", "4",
    "--time", "1",
    "--time", "1.5",
    "--time", "3",
  ], {
    cwd: path.join(repoRoot, "tools/noon-mcp"),
    env: { ...process.env, NOON_PREVIEW_RUNTIME_CONFIG: runtimeConfig },
    timeout: 150_000,
    maxBuffer: 1024 * 1024,
  });

  const summary = JSON.parse(stdout.trim());
  assert.equal(summary.outputDir, outputDir);
  assert.equal(summary.manifestPath, path.join(outputDir, "manifest.json"));
  assert.equal(summary.samples, 4);

  const manifest = JSON.parse(await readFile(summary.manifestPath, "utf8"));
  assert.equal(manifest.schema, 2);
  assert.equal(manifest.source.path, sourcePath);
  assert.equal(manifest.source.sha256, sourceSha256);
  assert.equal(manifest.loopDurationSeconds, 4);
  assert.equal(manifest.samples.length, 4);
  assert.equal(manifest.cleanup?.state, "closed");

  const expectedTimes = [0, 1, 1.5, 3];
  let sessionId = null;
  for (let index = 0; index < manifest.samples.length; index += 1) {
    const sample = manifest.samples[index];
    const png = await readFile(path.join(outputDir, sample.filename));
    const pngSha256 = hash(png);
    assert.equal(sample.artifact.mimeType, "image/png");
    assert.equal(sample.artifact.sha256, pngSha256);
    assert.equal(sample.artifact.byteLength, png.length);
    assert.equal(sample.snapshot.image?.sha256, pngSha256);
    assert.equal(sample.snapshot.frame?.requestedTime, expectedTimes[index]);
    assert.equal(sample.snapshot.frame?.publishedTime, expectedTimes[index]);
    assert.equal(sample.artifact.provenance.requestedTime, expectedTimes[index]);
    assert.equal(sample.artifact.provenance.publishedTime, expectedTimes[index]);
    assert.equal(sample.artifact.provenance.backend, sample.snapshot.frame?.rendererBackend);
    assert.equal(sample.artifact.provenance.sourceSha256, sourceSha256);
    assert.deepEqual(sample.artifact.provenance.build, expectedBuild);
    if (sessionId === null) sessionId = sample.artifact.provenance.sessionId;
    assert.equal(sample.artifact.provenance.sessionId, sessionId);
  }

  const sampleIndex = 2;
  const selected = manifest.samples[sampleIndex];
  const capturedTime = selected.artifact.provenance.publishedTime;
  const note = {
    interval: { start: Math.max(0, capturedTime - 0.1), end: capturedTime + 0.1 },
    region: { x: 0, y: 0, width: 1, height: 1 },
    observation: "Review the shape correspondence in this captured transition.",
    desiredChange: "Keep the geometric relationship clear throughout the transformation.",
    preserve: "Keep the existing final circle and authored timing.",
  };
  const notePath = path.join(root, "note.json");
  const feedbackOutput = path.join(root, "feedback");
  await writeFile(notePath, JSON.stringify(note), { flag: "wx" });
  // Feedback consumes the completed capture offline. No runtime config or
  // provider credentials are inherited by these child commands.
  const feedbackCommandOptions = { cwd: root, env: {}, timeout: 10_000, maxBuffer: 256 * 1024 };
  const { stdout: annotationOutput } = await execFileAsync(process.execPath, [
    feedbackCliPath, "annotate", "--preview", summary.manifestPath,
    "--sample", String(sampleIndex), "--source", sourcePath,
    "--input", notePath, "--output", feedbackOutput,
  ], feedbackCommandOptions);
  const annotation = JSON.parse(annotationOutput.trim());
  assert.equal(annotation.outputDir, feedbackOutput);
  assert.equal(annotation.feedbackPath, path.join(feedbackOutput, "feedback.json"));
  const feedback = JSON.parse(await readFile(annotation.feedbackPath, "utf8"));
  assert.deepEqual(feedback, annotation.feedback);
  assert.equal(feedback.kind, "noon-frame-feedback");
  assert.equal(feedback.schemaVersion, 1);
  assert.equal(feedback.status, "pending_review");
  assert.deepEqual(feedback.feedback, note);
  const { retentionMs: _retentionMs, ...capturedFrame } = selected.artifact;
  assert.deepEqual(feedback.frame, capturedFrame);
  const feedbackPng = await readFile(path.join(feedbackOutput, "frame.png"));
  assert.deepEqual(feedbackPng, await readFile(path.join(outputDir, selected.filename)),
    "feedback bundle must retain the actual captured PNG");
  assert.equal(hash(feedbackPng), selected.artifact.sha256);
  const feedbackSource = await readFile(path.join(feedbackOutput, "source.txt"));
  assert.deepEqual(feedbackSource, sourceBytes);
  assert.equal(hash(feedbackSource), feedback.frame.provenance.sourceSha256);

  const { stdout: currentOutput } = await execFileAsync(process.execPath, [
    feedbackCliPath, "check", "--feedback", annotation.feedbackPath, "--source", sourcePath,
  ], feedbackCommandOptions);
  assert.deepEqual(JSON.parse(currentOutput.trim()), {
    status: "pending_review", sourceStatus: "current",
    observedSourceSha256: sourceSha256, currentSourceSha256: sourceSha256,
  });

  const revisedSourcePath = path.join(root, "revised-source.py");
  const revisedSource = Buffer.concat([sourceBytes, Buffer.from("\n# Feedback source freshness probe.\n")]);
  await writeFile(revisedSourcePath, revisedSource, { flag: "wx" });
  await assert.rejects(execFileAsync(process.execPath, [
    feedbackCliPath, "check", "--feedback", annotation.feedbackPath, "--source", revisedSourcePath,
  ], feedbackCommandOptions), (error) => {
    assert.equal(error.code, 2, "stale feedback source must use the documented exit status");
    assert.deepEqual(JSON.parse(error.stdout.trim()), {
      status: "pending_review", sourceStatus: "stale",
      observedSourceSha256: sourceSha256, currentSourceSha256: hash(revisedSource),
    });
    return true;
  });
  assert.deepEqual(JSON.parse(await readFile(annotation.feedbackPath, "utf8")), feedback,
    "source checks must preserve the original pending feedback record");

  const { stdout: containers } = await execFileAsync("docker", [
    "ps", "--filter", "label=com.noon.preview=true", "--format", "{{.ID}}",
  ], { timeout: 10_000, maxBuffer: 64 * 1024 });
  assert.equal(containers.trim(), "", "CLI completion must leave no owned preview container running");

  console.log(JSON.stringify({
    ok: true,
    buildId: expectedBuild.buildId,
    sourceSha256,
    frameSha256: manifest.samples.map((sample) => sample.artifact.sha256),
    annotatedFrameSha256: feedback.frame.sha256,
    annotationStatus: feedback.status,
    feedbackSourceSnapshotVerified: true,
    staleSourceDetected: true,
  }));
} finally {
  await rm(root, { recursive: true, force: true });
}
