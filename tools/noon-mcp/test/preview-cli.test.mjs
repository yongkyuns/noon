import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { MAX_PREVIEW_CLI_SAMPLES, parsePreviewCliArgs, runPreviewCli } from "../src/preview-cli.mjs";

const png = Buffer.from([137,80,78,71,13,10,26,10,1,2,3,4]);
const pngSha256 = createHash("sha256").update(png).digest("hex");
const build = Object.freeze({
  engineRevision: "a".repeat(40),
  wasmSha256: "b".repeat(64),
  workerSha256: "c".repeat(64),
  buildId: "d".repeat(64),
});

function fakeService() {
  const calls = [];
  const scope = Object.freeze({ capability: true });
  const sessionId = "01234567-89ab-cdef-0123-456789abcdef";
  const artifacts = new Map();
  let sourceSha256 = null;
  let nextArtifact = 0;

  const snapshot = (time) => Object.freeze({
    state: "ready",
    sourceState: "running",
    authoredDuration: 4,
    frame: Object.freeze({ requestedTime: time, publishedTime: time, rendererBackend: "WebGL2" }),
    image: Object.freeze({ mimeType: "image/png", encodedBytes: png.length, sha256: pngSha256 }),
  });
  const put = (time) => {
    const descriptor = Object.freeze({
      id: `artifact-${nextArtifact++}`,
      mimeType: "image/png",
      width: 1,
      height: 1,
      byteLength: png.length,
      sha256: pngSha256,
      provenance: Object.freeze({
        sessionId,
        sourceSha256,
        build,
        requestedTime: time,
        publishedTime: time,
        backend: "WebGL2",
        sceneRevision: null,
        frameRevision: null,
      }),
      retentionMs: 300_000,
    });
    artifacts.set(descriptor.id, { descriptor, png: Buffer.from(png) });
    return { snapshot: snapshot(time), artifact: descriptor };
  };

  return {
    calls,
    openScope() { calls.push(["openScope"]); return scope; },
    async open(actualScope, source, options) {
      calls.push(["open", actualScope, source, options]);
      assert.equal(actualScope, scope);
      sourceSha256 = createHash("sha256").update(source, "utf8").digest("hex");
      return Object.freeze({ sessionId, ...put(0) });
    },
    async sampleFrames(actualScope, actualSessionId, times) {
      calls.push(["sampleFrames", actualScope, actualSessionId, [...times]]);
      assert.equal(actualScope, scope);
      assert.equal(actualSessionId, sessionId);
      return Object.freeze(times.map((time) => Object.freeze(put(time))));
    },
    getArtifact(actualScope, actualSessionId, artifactId) {
      calls.push(["getArtifact", actualScope, actualSessionId, artifactId]);
      assert.equal(actualScope, scope);
      assert.equal(actualSessionId, sessionId);
      const retained = artifacts.get(artifactId);
      if (!retained) throw new Error("artifact unavailable");
      return Object.freeze({ descriptor: retained.descriptor, png: Buffer.from(retained.png) });
    },
    async close(actualScope, actualSessionId, reason) {
      calls.push(["close", actualScope, actualSessionId, reason]);
      assert.equal(actualScope, scope);
      assert.equal(actualSessionId, sessionId);
      artifacts.clear();
      return Object.freeze({ state: "closed", cleanup: Object.freeze({ closed: true, cleanup: Object.freeze({ removed: true }) }) });
    },
    async closeScope(actualScope, reason) {
      calls.push(["closeScope", actualScope, reason]);
      assert.equal(actualScope, scope);
      return Object.freeze([]);
    },
    async dispose(reason) { calls.push(["dispose", reason]); return Object.freeze([]); },
  };
}

test("argument parser enforces bounded forward schedules within retained-frame budget", () => {
  assert.equal(MAX_PREVIEW_CLI_SAMPLES, 31);
  assert.deepEqual(parsePreviewCliArgs(["--source", "scene.py", "--time", "1", "--time", "1.5", "--loop-duration", "4"]), {
    sourcePath: "scene.py", outputDir: "noon-preview-output", loopDurationSeconds: 4, times: [1, 1.5], help: false,
  });
  assert.throws(() => parsePreviewCliArgs(["--source", "scene.py", "--time", "2", "--time", "1"]), /nondecreasing/);
  assert.throws(() => parsePreviewCliArgs(["--source", "scene.py", "--time", "601"]), /<= 600/);
  const tooMany = ["--source", "scene.py"];
  for (let index = 0; index < 32; index += 1) tooMany.push("--time", String(index));
  assert.throws(() => parsePreviewCliArgs(tooMany), /at most 31/);
  assert.throws(() => parsePreviewCliArgs([]), /--source is required/);
});

test("CLI uses one composed service scope and writes retained PNGs plus provenance manifest", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-preview-cli-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const sourceText = "from noon import *\n";
  await writeFile(path.join(root, "scene.py"), sourceText);
  const service = fakeService();
  const result = await runPreviewCli({
    argv: ["--source", "scene.py", "--output", "out", "--time", "0", "--time", "1.5"],
    cwd: root,
    serviceFactory: () => service,
  });

  assert.deepEqual(service.calls.map((entry) => entry[0]),
    ["openScope", "open", "getArtifact", "sampleFrames", "getArtifact", "close", "closeScope", "dispose"]);
  assert.deepEqual(service.calls.find((entry) => entry[0] === "sampleFrames")[3], [1.5]);
  assert.deepEqual(await readFile(path.join(root, "out/frame-000.png")), png);
  assert.deepEqual(await readFile(path.join(root, "out/frame-001.png")), png);

  const manifest = JSON.parse(await readFile(result.manifestPath, "utf8"));
  const sourceSha256 = createHash("sha256").update(sourceText).digest("hex");
  assert.equal(manifest.schema, 2);
  assert.equal(manifest.samples.length, 2);
  assert.equal(manifest.samples[1].snapshot.frame.requestedTime, 1.5);
  assert.equal(manifest.source.sha256, sourceSha256);
  for (const sample of manifest.samples) {
    assert.equal(sample.artifact.provenance.sourceSha256, sourceSha256);
    assert.deepEqual(sample.artifact.provenance.build, build);
    assert.equal(sample.artifact.sha256, pngSha256);
    assert.equal(sample.artifact.byteLength, png.length);
  }
  assert.equal(manifest.cleanup.cleanup.cleanup.removed, true);
});

test("CLI always awaits session, scope and service cleanup after sampling failure", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-preview-cli-fail-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFile(path.join(root, "scene.py"), "scene\n");
  const service = fakeService();
  service.sampleFrames = async (scope, sessionId, times) => {
    service.calls.push(["sampleFrames", scope, sessionId, [...times]]);
    throw new Error("sample failed");
  };
  await assert.rejects(runPreviewCli({
    argv: ["--source", "scene.py", "--output", "out", "--time", "1"],
    cwd: root,
    serviceFactory: () => service,
  }), /sample failed/);
  assert.deepEqual(service.calls.slice(-3).map((entry) => entry[0]), ["close", "closeScope", "dispose"]);
  assert.equal(service.calls.find((entry) => entry[0] === "close")[3], "preview CLI failed");
});

test("CLI fails closed if retained bytes disagree with the service artifact descriptor", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-preview-cli-mismatch-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFile(path.join(root, "scene.py"), "scene\n");
  const service = fakeService();
  const getArtifact = service.getArtifact.bind(service);
  service.getArtifact = (...args) => {
    const retained = getArtifact(...args);
    retained.png[retained.png.length - 1] ^= 0xff;
    return retained;
  };
  await assert.rejects(runPreviewCli({
    argv: ["--source", "scene.py", "--output", "out"], cwd: root, serviceFactory: () => service,
  }), /does not match the coherent service observation/);
  assert.deepEqual(service.calls.slice(-3).map((entry) => entry[0]), ["close", "closeScope", "dispose"]);
});
