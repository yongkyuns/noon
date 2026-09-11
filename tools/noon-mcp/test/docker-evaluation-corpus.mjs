import assert from "node:assert/strict";
import { execFile, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

import { loadEvaluationCorpus } from "../eval/corpus.mjs";
import { createDiscovery } from "../src/discovery.mjs";
import { loadPreviewRuntimeConfig } from "../src/preview-isolation.mjs";
import { AgentPreviewService } from "../src/preview-service.mjs";

const execFileAsync = promisify(execFile);
const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(here, "..");
const repoRoot = path.resolve(packageRoot, "../..");
const runtimeConfigPath = process.env.NOON_PREVIEW_RUNTIME_CONFIG;
if (!runtimeConfigPath) throw new Error("NOON_PREVIEW_RUNTIME_CONFIG is required for deterministic corpus qualification");
const python = process.env.NOON_PYTHON || execFileSync("python3", ["-I", "-S", "-c", "import sys; print(sys.executable)"], { encoding: "utf8" }).trim();
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

const corpus = await loadEvaluationCorpus();
const isolationConfig = await loadPreviewRuntimeConfig({ repoRoot, configPath: runtimeConfigPath });
const runtimeIdentity = JSON.parse(await readFile(path.join(repoRoot, "web/runtime-build-identity.json"), "utf8"));
const expectedBuild = Object.freeze({
  engineRevision: runtimeIdentity.sourceRevision ?? null,
  wasmSha256: runtimeIdentity.files?.wasm?.sha256,
  workerSha256: runtimeIdentity.files?.worker?.sha256,
  buildId: runtimeIdentity.buildId,
});
const discovery = await createDiscovery({ repoRoot, pythonExecutable: python });
const service = new AgentPreviewService({ isolationConfig });
const scope = service.openScope();
const evidence = [];

function assertBuild(build, label) {
  assert.deepEqual(build, expectedBuild, `${label} must report the browser-observed build identity`);
}

function assertFrame(entry, { task, time, sourceSha256, sessionId }) {
  const { snapshot, artifact } = entry;
  assert.equal(snapshot.state, "ready", `${task.id}@${time} must remain ready`);
  assert.equal(snapshot.frame?.requestedTime, time, `${task.id}@${time} requested time`);
  assert.equal(snapshot.frame?.publishedTime, time, `${task.id}@${time} published time`);
  assert.equal(snapshot.frame?.rendererBackend, task.expectedBackend, `${task.id}@${time} backend`);
  assert.ok(Number.isSafeInteger(snapshot.frame?.objectCount) && snapshot.frame.objectCount >= 0,
    `${task.id}@${time} must expose a semantic object-count observation`);
  assert.equal(artifact.mimeType, "image/png");
  assert.ok(Number.isSafeInteger(artifact.width) && artifact.width > 0);
  assert.ok(Number.isSafeInteger(artifact.height) && artifact.height > 0);
  assert.ok(Number.isSafeInteger(artifact.byteLength) && artifact.byteLength > 0);
  assert.match(artifact.sha256, /^[0-9a-f]{64}$/);
  assert.equal(artifact.provenance.sessionId, sessionId);
  assert.equal(artifact.provenance.sourceSha256, sourceSha256);
  assert.equal(artifact.provenance.requestedTime, time);
  assert.equal(artifact.provenance.publishedTime, time);
  assert.equal(artifact.provenance.backend, task.expectedBackend);
  assertBuild(artifact.provenance.build, `${task.id}@${time}`);

  const retained = service.getArtifact(scope, sessionId, artifact.id);
  assert.deepEqual(retained.descriptor, artifact, `${task.id}@${time} retained descriptor drifted`);
  assert.equal(retained.png.length, artifact.byteLength);
  assert.equal(sha256(retained.png), artifact.sha256, `${task.id}@${time} retained PNG hash mismatch`);
  return artifact.sha256;
}

function assertHashRelations(task, hashes) {
  for (const relation of task.hashRelations) {
    const left = hashes[relation.left], right = hashes[relation.right];
    if (relation.op === "equal") assert.equal(left, right,
      `${task.id} expected equal raster states at ${relation.left}/${relation.right}`);
    else assert.notEqual(left, right,
      `${task.id} expected distinct raster states at ${relation.left}/${relation.right}`);
  }
}

async function renderOnce(task) {
  const reference = await discovery.reference({ example: task.example });
  const sourceSha256 = reference.example.source_sha256;
  const opened = await service.open(scope, reference.source, { loopDurationSeconds: task.loopDurationSeconds });
  const { sessionId } = opened;
  try {
    const frames = [opened];
    if (task.sampleTimes.length > 1) {
      frames.push(...await service.sampleFrames(scope, sessionId, task.sampleTimes.slice(1)));
    }
    assert.equal(frames.length, task.sampleTimes.length);
    const hashes = frames.map((entry, index) => assertFrame(entry, {
      task,
      time: task.sampleTimes[index],
      sourceSha256,
      sessionId,
    }));
    const final = frames.at(-1).snapshot;
    assert.equal(final.sourceState, "completed", `${task.id} source continuation must complete`);
    assert.equal(final.authoredDuration, task.expectedAuthoredDuration, `${task.id} authored duration`);
    assert.equal(final.frame.objectCount, task.expectedFinalObjectCount, `${task.id} final object count`);
    assertHashRelations(task, hashes);
    evidence.push({ id: task.id, sessionId, sourceSha256, hashes, finalObjectCount: final.frame.objectCount,
      authoredDuration: final.authoredDuration, backend: final.frame.rendererBackend });
    return hashes;
  } finally {
    const closed = await service.close(scope, sessionId, `${task.id} deterministic evaluation complete`);
    assert.equal(closed.state, "closed");
    assert.throws(() => service.inspect(scope, sessionId), /stale or cross-scope/,
      `${task.id} handle must be stale after close`);
  }
}

async function verifyUnsupported(task) {
  const report = await discovery.capabilities({ examples: [task.example] });
  const record = report.examples[task.example];
  assert.equal(record.status, task.expectedStatus, `${task.id} support status changed`);
  const features = new Set(record.features ?? []);
  for (const feature of task.requiredFeatures) assert.ok(features.has(feature), `${task.id} lost ${feature}`);
  assert.equal(record.runtime_verified, false);
  await assert.rejects(discovery.reference({ example: task.example }), /not ready/);
  evidence.push({ id: task.id, status: record.status, features: task.requiredFeatures });
}

async function verifyCancellation(task) {
  const source = await readFile(path.join(packageRoot, task.sourcePath), "utf8");
  const opened = await service.open(scope, source, { loopDurationSeconds: task.loopDurationSeconds });
  assert.equal(opened.snapshot.frame?.objectCount, task.expectedInitialObjectCount,
    `${task.id} must publish a useful first frame before the source stalls`);
  const controller = new AbortController();
  const reason = new Error("deterministic evaluation cancellation");
  const pending = service.sample(scope, opened.sessionId, task.sampleTime, { signal: controller.signal });
  pending.catch(() => {});
  const timer = setTimeout(() => controller.abort(reason), task.abortAfterMs);
  try {
    await assert.rejects(pending, /deterministic evaluation cancellation/);
  } finally {
    clearTimeout(timer);
  }
  assert.throws(() => service.inspect(scope, opened.sessionId), /stale or cross-scope/,
    "canceled session must be retired authoritatively");
  assert.equal(service.stats.sessions, 0, "cancellation must release the runner session before recovery");
  evidence.push({ id: task.id, canceled: true, sourceSha256: sha256(Buffer.from(source, "utf8")) });
}

let baselineRepeatTask = null;
let baselineRepeatHashes = null;
try {
  for (const task of corpus.tasks.filter((item) => item.kind === "render")) {
    const hashes = await renderOnce(task);
    if (task.repeatFresh > 1) {
      baselineRepeatTask = task;
      baselineRepeatHashes = hashes;
    }
  }
  for (const task of corpus.tasks.filter((item) => item.kind === "capability")) await verifyUnsupported(task);
  const cancellation = corpus.tasks.find((item) => item.kind === "cancellation");
  assert.ok(cancellation, "corpus must include cancellation");
  await verifyCancellation(cancellation);

  assert.ok(baselineRepeatTask && baselineRepeatHashes, "corpus must include a repeated fresh render task");
  for (let run = 1; run < baselineRepeatTask.repeatFresh; run += 1) {
    const freshHashes = await renderOnce(baselineRepeatTask);
    assert.deepEqual(freshHashes, baselineRepeatHashes,
      `${baselineRepeatTask.id} fresh run ${run + 1} must reproduce exact raster evidence after cancellation recovery`);
  }

  assert.equal(service.stats.sessions, 0);
  assert.equal(service.stats.artifacts.artifacts, 0);
  const { stdout: containers } = await execFileAsync("docker", [
    "ps", "--filter", "label=com.noon.preview=true", "--format", "{{.ID}}",
  ], { timeout: 10_000, maxBuffer: 64 * 1024 });
  assert.equal(containers.trim(), "", "deterministic corpus must leave no owned preview container running");

  console.log(JSON.stringify({
    ok: true,
    schema: corpus.schema,
    kind: corpus.kind,
    buildId: expectedBuild.buildId,
    engineRevision: expectedBuild.engineRevision,
    tasks: evidence,
    cancellationRecovered: true,
    repeatedRunEquivalent: true,
  }));
} finally {
  await service.closeScope(scope, "deterministic corpus complete").catch(() => {});
  await service.dispose("deterministic corpus shutdown").catch(() => {});
  discovery.close();
}
