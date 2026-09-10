import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const execFileAsync = promisify(execFile);
const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../../..");
const cliPath = path.join(repoRoot, "tools/noon-mcp/bin/noon-preview.mjs");
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

  const { stdout: containers } = await execFileAsync("docker", [
    "ps", "--filter", "label=com.noon.preview=true", "--format", "{{.ID}}",
  ], { timeout: 10_000, maxBuffer: 64 * 1024 });
  assert.equal(containers.trim(), "", "CLI completion must leave no owned preview container running");

  console.log(JSON.stringify({
    ok: true,
    buildId: expectedBuild.buildId,
    sourceSha256,
    frameSha256: manifest.samples.map((sample) => sample.artifact.sha256),
  }));
} finally {
  await rm(root, { recursive: true, force: true });
}
