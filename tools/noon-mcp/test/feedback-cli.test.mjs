import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { access, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { FrameArtifactStore } from "../../../scripts/agent-preview-artifacts.mjs";
import { parseFeedbackCliArgs, runFeedbackCli } from "../src/feedback-cli.mjs";
import { runPreviewCli } from "../src/preview-cli.mjs";

const execFileAsync = promisify(execFile);
const cli = fileURLToPath(new URL("../bin/noon-feedback.mjs", import.meta.url));
// Genuine tiny PNG fixture; this suite does not render a scene.
const PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgAAIAAAUAAXpeqz8AAAAASUVORK5CYII=", "base64");
const BUILD = { engineRevision: "a".repeat(40), wasmSha256: "b".repeat(64), workerSha256: "c".repeat(64), buildId: "fixture-build" };
const ORIGINAL = "from noon import *\n# Original educational scene fixture\n";
const REVISED = "from noon import *\n# Revised educational scene fixture\n";
const NOTE = {
  interval: { start: 1, end: 2 }, region: { x: 0.2, y: 0.2, width: 0.5, height: 0.4 },
  observation: "The contributions disappear while the sum reaches zero.",
  desiredChange: "Keep the contributions visible.", preserve: "Distinct inputs can cancel.",
};
const sha256 = (value) => createHash("sha256").update(value).digest("hex");

function fakeService(sessionId) {
  const scope = Object.freeze({});
  const store = new FrameArtifactStore();
  let frames;
  const put = (time) => {
    const descriptor = frames.putFrame({
      png: PNG, requestedTime: time, publishedTime: time,
      backend: "fixture", sceneRevision: null, frameRevision: null,
    });
    return { artifact: descriptor, snapshot: {
      state: "ready", frame: { requestedTime: time, publishedTime: time, rendererBackend: "fixture" },
      buildIdentity: {
        sourceRevision: BUILD.engineRevision, buildId: BUILD.buildId,
        files: { wasm: { sha256: BUILD.wasmSha256 }, worker: { sha256: BUILD.workerSha256 } },
      },
      image: {
        mimeType: descriptor.mimeType, width: descriptor.width, height: descriptor.height,
        encodedBytes: descriptor.byteLength, sha256: descriptor.sha256,
      },
    } };
  };
  return {
    openScope: () => scope,
    async open(actualScope, source) {
      assert.equal(actualScope, scope);
      frames = store.openScope({ sessionId, sourceSha256: sha256(source), build: BUILD });
      return { sessionId, ...put(0) };
    },
    async sampleFrames(actualScope, actualSession, times) {
      assert.equal(actualScope, scope); assert.equal(actualSession, sessionId);
      return times.map(put);
    },
    getArtifact(actualScope, actualSession, id) {
      assert.equal(actualScope, scope); assert.equal(actualSession, sessionId);
      return frames.getFrame(id);
    },
    async close() { frames.close(); return { state: "closed" }; },
    async closeScope() { return []; },
    async dispose() { store.dispose(); return []; },
  };
}

async function exportPreview(root, output, { session = "original-run", time = 1.5 } = {}) {
  return runPreviewCli({
    cwd: root, argv: ["--source", "scene.py", "--output", output, "--time", String(time)],
    serviceFactory: () => fakeService(session),
  });
}

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-frame-feedback-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFile(path.join(root, "scene.py"), ORIGINAL);
  await writeFile(path.join(root, "note.json"), JSON.stringify(NOTE));
  await exportPreview(root, "before");
  return root;
}

function annotate(root, output = "review") {
  return runFeedbackCli({ cwd: root, argv: [
    "annotate", "--preview", "before/manifest.json", "--sample", "1", "--source", "scene.py",
    "--input", "note.json", "--output", output,
  ] });
}

async function modifyJson(filename, change) {
  const value = JSON.parse(await readFile(filename, "utf8"));
  change(value);
  await writeFile(filename, JSON.stringify(value));
}

test("offline feedback exports verified evidence, detects source edits, and compares independent observations", async (t) => {
  const root = await fixture(t);
  const result = await annotate(root);
  const originalFeedbackBytes = await readFile(result.feedbackPath);
  assert.deepEqual(JSON.parse(originalFeedbackBytes), result.feedback);
  assert.deepEqual(await readFile(path.join(result.outputDir, "frame.png")), PNG);
  assert.equal(await readFile(path.join(result.outputDir, "source.txt"), "utf8"), ORIGINAL);
  assert.equal(result.feedback.frame.provenance.sourceSha256, sha256(ORIGINAL));
  assert.deepEqual(result.feedback.frame.provenance.build, BUILD);
  assert.equal(result.feedback.frame.provenance.sessionId, "original-run");
  assert.equal(result.feedback.frame.provenance.publishedTime, 1.5);

  const check = () => runFeedbackCli({ cwd: root, argv: ["check", "--feedback", "review/feedback.json", "--source", "scene.py"] });
  assert.equal((await check()).sourceStatus, "current");
  await writeFile(path.join(root, "scene.py"), REVISED);
  const stale = await check();
  assert.deepEqual(stale, {
    status: "pending_review", sourceStatus: "stale",
    observedSourceSha256: sha256(ORIGINAL), currentSourceSha256: sha256(REVISED),
  });
  assert.deepEqual(await readFile(result.feedbackPath), originalFeedbackBytes);

  await exportPreview(root, "after", { session: "revised-run", time: 3 });
  const compared = await runFeedbackCli({ cwd: root, argv: [
    "compare", "--feedback", "review/feedback.json", "--candidate", "after/manifest.json",
    "--sample", "1", "--source", "scene.py", "--output", "comparison",
  ] });
  assert.deepEqual(JSON.parse(await readFile(compared.comparisonPath, "utf8")), compared.comparison);
  assert.equal(compared.comparison.status, "pending_review");
  assert.equal(compared.comparison.intervalAssociation, "unverified");
  assert.deepEqual(compared.comparison.before, result.feedback);
  assert.deepEqual(compared.comparison.before.feedback.interval, NOTE.interval);
  assert.equal(compared.comparison.after.provenance.sessionId, "revised-run");
  assert.equal(compared.comparison.after.provenance.sourceSha256, sha256(REVISED));
  assert.equal(compared.comparison.after.provenance.publishedTime, 3);
  assert.equal(await readFile(path.join(compared.outputDir, "before-source.txt"), "utf8"), ORIGINAL);
  assert.equal(await readFile(path.join(compared.outputDir, "after-source.txt"), "utf8"), REVISED);
  assert.deepEqual(await readFile(path.join(compared.outputDir, "before.png")), PNG);
  assert.deepEqual(await readFile(path.join(compared.outputDir, "after.png")), PNG);
  assert.deepEqual(await readFile(result.feedbackPath), originalFeedbackBytes);
});

test("a manifest cannot select the source file and stale new annotations fail without creating output", async (t) => {
  const root = await fixture(t);
  await modifyJson(path.join(root, "before/manifest.json"), (manifest) => {
    manifest.source.path = "/does/not/exist/this-path-must-not-be-read";
  });
  await annotate(root);
  await writeFile(path.join(root, "scene.py"), REVISED);
  await assert.rejects(annotate(root, "stale-review"), /association is stale/);
  await assert.rejects(access(path.join(root, "stale-review")), { code: "ENOENT" });
});

test("preview bytes, descriptor dimensions, and coherent snapshot identity are checked", async (t) => {
  for (const mutation of [
    "png", "dimensions", "snapshot", "source-identity", "build", "build-id", "build-files",
    "image", "image-size", "image-dimensions", "missing-build", "missing-image",
  ]) {
    await t.test(mutation, async (t) => {
      const root = await fixture(t);
      if (mutation === "png") {
        const changed = Buffer.from(PNG); changed[45] ^= 1;
        await writeFile(path.join(root, "before/frame-001.png"), changed);
      } else {
        await modifyJson(path.join(root, "before/manifest.json"), (manifest) => {
          const sample = manifest.samples[1];
          if (mutation === "dimensions") sample.artifact.width = 2;
          if (mutation === "snapshot") sample.snapshot.frame.publishedTime = 1.75;
          if (mutation === "source-identity") sample.artifact.provenance.sourceSha256 = "f".repeat(64);
          if (mutation === "build") sample.snapshot.buildIdentity.sourceRevision = "e".repeat(40);
          if (mutation === "build-id") sample.snapshot.buildIdentity.buildId = "different-build";
          if (mutation === "build-files") sample.snapshot.buildIdentity.files.worker.sha256 = "e".repeat(64);
          if (mutation === "image") sample.snapshot.image.sha256 = "e".repeat(64);
          if (mutation === "image-size") sample.snapshot.image.encodedBytes += 1;
          if (mutation === "image-dimensions") sample.snapshot.image.width += 1;
          if (mutation === "missing-build") delete sample.snapshot.buildIdentity;
          if (mutation === "missing-image") delete sample.snapshot.image;
        });
      }
      await assert.rejects(annotate(root), /does not match/);
      await assert.rejects(access(path.join(root, "review")), { code: "ENOENT" });
    });
  }
});

test("persisted feedback rejects image, source, and resolution tampering", async (t) => {
  for (const mutation of ["image", "source", "resolved"]) {
    await t.test(mutation, async (t) => {
      const root = await fixture(t);
      await annotate(root);
      if (mutation === "image") {
        const changed = Buffer.from(PNG); changed[45] ^= 1;
        await writeFile(path.join(root, "review/frame.png"), changed);
      } else if (mutation === "source") {
        await writeFile(path.join(root, "review/source.txt"), REVISED);
      } else {
        await modifyJson(path.join(root, "review/feedback.json"), (record) => { record.status = "resolved"; });
      }
      await assert.rejects(runFeedbackCli({ cwd: root, argv: [
        "check", "--feedback", "review/feedback.json", "--source", "scene.py",
      ] }));
    });
  }
});

test("preview filenames cannot traverse directories or follow a symlink outside the capture directory", async (t) => {
  for (const filename of ["../outside.png", "/outside.png", "nested/frame.png", "escaped.png"]) {
    await t.test(filename, async (t) => {
      const root = await fixture(t);
      await writeFile(path.join(root, "outside.png"), PNG);
      if (filename === "escaped.png") await symlink("../outside.png", path.join(root, "before/escaped.png"));
      await modifyJson(path.join(root, "before/manifest.json"), (manifest) => { manifest.samples[1].filename = filename; });
      await assert.rejects(annotate(root), /confined PNG filename|escapes its artifact directory/);
      await assert.rejects(access(path.join(root, "review")), { code: "ENOENT" });
    });
  }
});

test("feedback bundle image and source snapshots cannot escape through symlinks", async (t) => {
  for (const filename of ["frame.png", "source.txt"]) {
    await t.test(filename, async (t) => {
      const root = await fixture(t);
      await annotate(root);
      await writeFile(path.join(root, filename), filename === "frame.png" ? PNG : ORIGINAL);
      await rm(path.join(root, "review", filename));
      await symlink(`../${filename}`, path.join(root, "review", filename));
      await assert.rejects(runFeedbackCli({ cwd: root, argv: [
        "check", "--feedback", "review/feedback.json", "--source", "scene.py",
      ] }), /escapes its artifact directory/);
    });
  }
});

test("occupied review output is preserved and malformed or oversized source is rejected", async (t) => {
  const root = await fixture(t);
  await mkdir(path.join(root, "review"));
  await writeFile(path.join(root, "review/accepted.txt"), "accepted evidence");
  await assert.rejects(annotate(root), { code: "EEXIST" });
  assert.equal(await readFile(path.join(root, "review/accepted.txt"), "utf8"), "accepted evidence");
  for (const bytes of [Buffer.from([0xff]), Buffer.alloc(1_000_001, 65)]) {
    await writeFile(path.join(root, "scene.py"), bytes);
    await assert.rejects(annotate(root, "invalid-review"));
    await assert.rejects(access(path.join(root, "invalid-review")), { code: "ENOENT" });
  }
});

test("CLI parser bounds sample selection and rejects ambiguous invocation", () => {
  const valid = ["annotate", "--preview", "manifest.json", "--source", "scene.py", "--input", "note.json", "--output", "review"];
  assert.equal(parseFeedbackCliArgs(valid).sample, 0);
  for (const argv of [
    [], ["resolve"], valid.concat("--sample", "32"), valid.concat("--sample", "1.5"),
    valid.concat("--sample", "1", "--sample", "2"), valid.concat("--source", "other.py"),
    valid.concat("--resolved", "true"), ["check", "--feedback", "feedback.json"],
  ]) assert.throws(() => parseFeedbackCliArgs(argv));
});

test("executable exposes help and uses exit code 2 for stale source without rewriting feedback", async (t) => {
  const { stdout: help } = await execFileAsync(process.execPath, [cli, "--help"]);
  assert.match(help, /noon-feedback annotate/);
  const root = await fixture(t);
  const result = await annotate(root);
  const before = await readFile(result.feedbackPath);
  const args = [cli, "check", "--feedback", "review/feedback.json", "--source", "scene.py"];
  const current = await execFileAsync(process.execPath, args, { cwd: root });
  assert.equal(JSON.parse(current.stdout).sourceStatus, "current");
  await writeFile(path.join(root, "scene.py"), REVISED);
  await assert.rejects(execFileAsync(process.execPath, args, { cwd: root }), (error) => {
    assert.equal(error.code, 2);
    assert.equal(JSON.parse(error.stdout).sourceStatus, "stale");
    assert.equal(error.stderr, "");
    return true;
  });
  assert.deepEqual(await readFile(result.feedbackPath), before);
});
