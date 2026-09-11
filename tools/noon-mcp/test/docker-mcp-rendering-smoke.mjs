import assert from "node:assert/strict";
import { execFile, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

import { Client } from "@modelcontextprotocol/client";
import { StdioClientTransport } from "@modelcontextprotocol/client/stdio";

const execFileAsync = promisify(execFile);
const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../../..");
const serverPath = path.join(repoRoot, "tools/noon-mcp/src/server.mjs");
const cliPath = path.join(repoRoot, "tools/noon-mcp/bin/noon-preview.mjs");
const sourcePath = path.join(repoRoot, "web/python/examples/manim_parity_square_to_circle.py");
const runtimeConfig = process.env.NOON_PREVIEW_RUNTIME_CONFIG;
if (!runtimeConfig) throw new Error("NOON_PREVIEW_RUNTIME_CONFIG is required for Docker MCP rendering smoke");
const python = execFileSync("python3", ["-c", "import sys; print(sys.executable)"], { encoding: "utf8" }).trim();

const sourceBytes = await readFile(sourcePath);
const source = sourceBytes.toString("utf8");
const revisedSource = source.replace("PINK", "BLUE");
assert.notEqual(revisedSource, source, "revision smoke requires a real source edit");
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const sourceSha256 = hash(sourceBytes);
const revisedSourceSha256 = hash(Buffer.from(revisedSource, "utf8"));
const runtimeIdentity = JSON.parse(await readFile(path.join(repoRoot, "web/runtime-build-identity.json"), "utf8"));
const expectedBuild = Object.freeze({
  engineRevision: runtimeIdentity.sourceRevision ?? null,
  wasmSha256: runtimeIdentity.files?.wasm?.sha256,
  workerSha256: runtimeIdentity.files?.worker?.sha256,
  buildId: runtimeIdentity.buildId,
});

function assertBuildShape(build) {
  assert.match(build.buildId, /^[0-9a-f]{64}$/);
  assert.match(build.wasmSha256, /^[0-9a-f]{64}$/);
  assert.match(build.workerSha256, /^[0-9a-f]{64}$/);
  if (build.engineRevision !== null) assert.match(build.engineRevision, /^[0-9a-f]{40}$/);
}
assertBuildShape(expectedBuild);

async function connectClient(label) {
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [serverPath],
    env: {
      ...process.env,
      NOON_REPO: repoRoot,
      NOON_PYTHON: python,
      NOON_PREVIEW_RUNTIME_CONFIG: runtimeConfig,
    },
    stderr: "pipe",
  });
  const client = new Client({ name: `noon-rendering-smoke-${label}`, version: "0.1.0" });
  await client.connect(transport);
  return client;
}

function assertOk(result, label) {
  assert.notEqual(result.isError, true, `${label} unexpectedly failed: ${JSON.stringify(result.structuredContent ?? result.content)}`);
  assert.ok(result.structuredContent && typeof result.structuredContent === "object", `${label} must return structured content`);
  assert.equal(result.content?.[0]?.type, "text", `${label} must begin with structured JSON text`);
  assert.deepEqual(JSON.parse(result.content[0].text), result.structuredContent, `${label} text and structured content must agree`);
}

function assertArtifact(artifact, { sessionId, expectedSourceSha256, expectedTime, expectedBackend }) {
  assert.equal(artifact.mimeType, "image/png");
  assert.match(artifact.sha256, /^[0-9a-f]{64}$/);
  assert.ok(Number.isSafeInteger(artifact.byteLength) && artifact.byteLength > 0);
  assert.equal(artifact.provenance.sessionId, sessionId);
  assert.equal(artifact.provenance.sourceSha256, expectedSourceSha256);
  assert.equal(artifact.provenance.requestedTime, expectedTime);
  assert.equal(artifact.provenance.publishedTime, expectedTime);
  if (expectedBackend !== undefined) assert.equal(artifact.provenance.backend, expectedBackend);
  assert.deepEqual(artifact.provenance.build, expectedBuild);
}

function imageHashes(result, artifacts, expectations) {
  const images = result.content.filter((item) => item.type === "image");
  assert.equal(images.length, artifacts.length, "each retained artifact must have one MCP image block");
  return images.map((image, index) => {
    assert.equal(image.mimeType, "image/png");
    const bytes = Buffer.from(image.data, "base64");
    assert.ok(bytes.length > 1000, "real rendered PNG must be non-trivial");
    const sha256 = hash(bytes);
    const artifact = artifacts[index];
    assertArtifact(artifact, expectations[index]);
    assert.equal(artifact.sha256, sha256, "MCP image bytes must match retained artifact identity");
    assert.equal(artifact.byteLength, bytes.length, "MCP image byte length must match retained artifact identity");
    return sha256;
  });
}

async function renderRun(client, actualSource, expectedSourceSha256) {
  const opened = await client.callTool({
    name: "noon_open_scene",
    arguments: { source: actualSource, loopDurationSeconds: 4 },
  }, { timeout: 120_000 });
  assertOk(opened, "open_scene");
  const sessionId = opened.structuredContent.session;
  assert.match(sessionId, /^[A-Za-z0-9._~-]{16,128}$/);
  assert.equal(opened.structuredContent.snapshot.frame.requestedTime, 0);
  const hashes = imageHashes(opened, [opened.structuredContent.artifact], [{
    sessionId,
    expectedSourceSha256,
    expectedTime: 0,
    expectedBackend: opened.structuredContent.snapshot.frame.rendererBackend,
  }]);

  const sampled = await client.callTool({
    name: "noon_sample_frames",
    arguments: { session: sessionId, times: [1, 1.5, 3] },
  }, { timeout: 120_000 });
  assertOk(sampled, "sample_frames");
  assert.equal(sampled.structuredContent.session, sessionId);
  assert.deepEqual(sampled.structuredContent.frames.map((frame) => frame.snapshot.frame.requestedTime), [1, 1.5, 3]);
  hashes.push(...imageHashes(
    sampled,
    sampled.structuredContent.frames.map((frame) => frame.artifact),
    sampled.structuredContent.frames.map((frame) => ({
      sessionId,
      expectedSourceSha256,
      expectedTime: frame.snapshot.frame.requestedTime,
      expectedBackend: frame.snapshot.frame.rendererBackend,
    })),
  ));
  return { sessionId, hashes };
}

async function closeScene(client, sessionId) {
  const closed = await client.callTool({ name: "noon_close_scene", arguments: { session: sessionId } });
  assertOk(closed, "close_scene");
  assert.equal(closed.structuredContent.session, sessionId);
  assert.equal(closed.structuredContent.closed.state, "closed");
}

async function ownedContainers() {
  const { stdout } = await execFileAsync("docker", [
    "ps", "--filter", "label=com.noon.preview=true", "--format", "{{.ID}}",
  ], { timeout: 10_000, maxBuffer: 64 * 1024 });
  return stdout.trim().split(/\s+/).filter(Boolean);
}

async function waitForNoOwnedContainers() {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    if ((await ownedContainers()).length === 0) return;
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  assert.deepEqual(await ownedContainers(), [], "owned preview containers must be removed within the cleanup bound");
}

const client = await connectClient("primary");
let primaryClosed = false;
try {
  const listed = await client.listTools();
  assert.deepEqual(listed.tools.map((tool) => tool.name).sort(), [
    "noon_capabilities",
    "noon_close_scene",
    "noon_inspect",
    "noon_open_scene",
    "noon_reference",
    "noon_sample_frames",
  ]);

  const sourceFailure = await client.callTool({
    name: "noon_open_scene",
    arguments: {
      source: "from noon import *\nraise RuntimeError('mcp-source-error-probe')\n",
      loopDurationSeconds: 4,
    },
  }, { timeout: 120_000 });
  assert.equal(sourceFailure.isError, true, "real source execution failure must remain an MCP tool error");
  assert.deepEqual(sourceFailure.content.map((item) => item.type), ["text"],
    "source execution failure must not invent image content");
  assert.ok(sourceFailure.structuredContent?.error && typeof sourceFailure.structuredContent.error.message === "string",
    "source execution failure must retain bounded structured diagnostics");
  assert.ok(sourceFailure.structuredContent.error.message.length > 0 && sourceFailure.structuredContent.error.message.length <= 1200);
  await waitForNoOwnedContainers();

  const baseline = await renderRun(client, source, sourceSha256);

  const intruder = await connectClient("cross-transport");
  try {
    const crossTransport = await intruder.callTool({
      name: "noon_inspect",
      arguments: { session: baseline.sessionId },
    });
    assert.equal(crossTransport.isError, true, "session handles must not cross transport-owned service scopes");
  } finally {
    await intruder.close();
  }

  const inspected = await client.callTool({ name: "noon_inspect", arguments: { session: baseline.sessionId } });
  assertOk(inspected, "inspect");
  assert.deepEqual(inspected.content.map((item) => item.type), ["text"]);
  assert.equal(inspected.structuredContent.snapshot.frame.requestedTime, 3);
  assert.equal(inspected.structuredContent.artifact.sha256, baseline.hashes.at(-1));
  assert.equal(inspected.structuredContent.retainedFrames, 4);

  const tooMany = await client.callTool({
    name: "noon_sample_frames",
    arguments: { session: baseline.sessionId, times: Array(32).fill(3) },
  });
  assert.equal(tooMany.isError, true, "schema must reject a sample batch larger than the retained-frame bound");
  const afterRejectedBatch = await client.callTool({ name: "noon_inspect", arguments: { session: baseline.sessionId } });
  assertOk(afterRejectedBatch, "inspect after rejected batch");
  assert.equal(afterRejectedBatch.structuredContent.retainedFrames, 4);
  assert.equal(afterRejectedBatch.structuredContent.snapshot.frame.requestedTime, 3);

  await closeScene(client, baseline.sessionId);
  const stale = await client.callTool({ name: "noon_inspect", arguments: { session: baseline.sessionId } });
  assert.equal(stale.isError, true, "closed session handle must become stale");

  const fresh = await renderRun(client, source, sourceSha256);
  assert.deepEqual(fresh.hashes, baseline.hashes, "fresh MCP session must reproduce deterministic PNG identities");
  await closeScene(client, fresh.sessionId);

  const revised = await renderRun(client, revisedSource, revisedSourceSha256);
  assert.ok(revised.hashes.some((value, index) => value !== baseline.hashes[index]),
    "revised source must produce at least one different rendered frame");
  await closeScene(client, revised.sessionId);

  const stuckSource = "from noon import *\nclass Stuck(Scene):\n    def construct(self):\n        self.add(Circle())\n        self.wait(0.1)\n        while True:\n            pass\n";
  const stuckOpened = await client.callTool({
    name: "noon_open_scene",
    arguments: { source: stuckSource, loopDurationSeconds: 4 },
  }, { timeout: 120_000 });
  assertOk(stuckOpened, "stuck first-frame open_scene");
  const stuckSessionId = stuckOpened.structuredContent.session;
  assert.equal(stuckOpened.structuredContent.snapshot.frame.requestedTime, 0,
    "open_scene must preserve first-frame readiness instead of waiting for source completion");

  const controller = new AbortController();
  const cancellationReason = new DOMException("noon-mcp-cancel-probe", "AbortError");
  const pending = client.callTool({
    name: "noon_sample_frames",
    arguments: { session: stuckSessionId, times: [0.2] },
  }, { signal: controller.signal, timeout: 30_000 });
  const abortTimer = setTimeout(() => controller.abort(cancellationReason), 750);
  let cancelled = false;
  try {
    const result = await pending;
    cancelled = result.isError === true;
  } catch (error) {
    cancelled = true;
    // @modelcontextprotocol/client 2.0.0 wraps deliberate AbortSignal rejection
    // in SdkError/REQUEST_TIMEOUT. Match the explicit reason instead of its
    // current wrapper type so this remains a cancellation proof after SDK fixes.
    assert.match(String(error?.message ?? error), /noon-mcp-cancel-probe/i,
      "client cancellation rejection must preserve the explicit abort reason");
  } finally {
    clearTimeout(abortTimer);
  }
  assert.equal(controller.signal.aborted, true, "cancellation probe must actually abort the client request");
  assert.equal(cancelled, true, "canceled sample_frames must never report success");
  await waitForNoOwnedContainers();
  const canceledStale = await client.callTool({ name: "noon_inspect", arguments: { session: stuckSessionId } });
  assert.equal(canceledStale.isError, true, "canceled sampling must retire and stale the owned session");

  const recovery = await renderRun(client, source, sourceSha256);
  await closeScene(client, recovery.sessionId);
  assert.deepEqual(recovery.hashes, baseline.hashes, "same MCP transport must recover after cancellation");

  const cliRoot = await mkdtemp(path.join(os.tmpdir(), "noon-mcp-cli-equivalence-"));
  try {
    const outputDir = path.join(cliRoot, "frames");
    await execFileAsync(process.execPath, [
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
    const manifest = JSON.parse(await readFile(path.join(outputDir, "manifest.json"), "utf8"));
    assert.deepEqual(manifest.samples.map((sample) => sample.artifact.sha256), baseline.hashes,
      "MCP and shared CLI must produce the same deterministic frame identities");
    assert.ok(manifest.samples.every((sample) => sample.artifact.provenance.sourceSha256 === sourceSha256));
    assert.ok(manifest.samples.every((sample) => JSON.stringify(sample.artifact.provenance.build) === JSON.stringify(expectedBuild)));
  } finally {
    await rm(cliRoot, { recursive: true, force: true });
  }

  await client.close();
  primaryClosed = true;
  await waitForNoOwnedContainers();

  const disconnectClient = await connectClient("disconnect-cleanup");
  const disconnected = await disconnectClient.callTool({
    name: "noon_open_scene",
    arguments: { source, loopDurationSeconds: 4 },
  }, { timeout: 120_000 });
  assertOk(disconnected, "disconnect cleanup open_scene");
  assert.equal((await ownedContainers()).length, 1, "open scene should own one isolated container before disconnect");
  await disconnectClient.close();
  await waitForNoOwnedContainers();

  console.log(JSON.stringify({
    ok: true,
    buildId: expectedBuild.buildId,
    sourceSha256,
    revisedSourceSha256,
    baselineFrameSha256: baseline.hashes,
    revisedFrameSha256: revised.hashes,
    sourceFailureRecovered: true,
    cancellationRecovered: true,
    disconnectCleanup: true,
    cliEquivalent: true,
  }));
} finally {
  if (!primaryClosed) await client.close().catch(() => {});
  await waitForNoOwnedContainers().catch(() => {});
}
