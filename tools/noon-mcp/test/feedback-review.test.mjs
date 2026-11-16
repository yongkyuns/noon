import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { access, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { FrameArtifactStore } from "../../../scripts/agent-preview-artifacts.mjs";
import { runFeedbackCli } from "../src/feedback-cli.mjs";
import { buildFrameFeedback } from "../src/frame-feedback.mjs";
import { runPreviewCli } from "../src/preview-cli.mjs";

// Actual PNG bytes used only as synthetic artifact evidence. No scene is run.
const PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgAAIAAAUAAXpeqz8AAAAASUVORK5CYII=", "base64");
const SOURCE = "from noon import *\n# Synthetic review fixture; not rendered.\n";
const BUILD = {
  engineRevision: "a".repeat(40), wasmSha256: "b".repeat(64),
  workerSha256: "c".repeat(64), buildId: "synthetic-review-build",
};
const TIME = 1.2345678901234567;
const INPUT = {
  interval: { start: 1, end: 2 }, region: { x: 0.2, y: 0.2, width: 0.6, height: 0.6 },
  observation: "Both inputs disappear as their sum reaches zero.",
  desiredChange: "Keep the inputs visible and change only the sum.",
  preserve: "Keep magnitudes, labels, scale, and accepted speech.",
};
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");

function fakeService(build) {
  const scope = Object.freeze({});
  const sessionId = "synthetic-review-session";
  const store = new FrameArtifactStore();
  let frames;
  const put = (time) => {
    const artifact = frames.putFrame({
      png: PNG, requestedTime: time, publishedTime: time,
      backend: "fixture", sceneRevision: null, frameRevision: null,
    });
    return { artifact, snapshot: {
      state: "ready", frame: { requestedTime: time, publishedTime: time, rendererBackend: "fixture" },
      buildIdentity: {
        sourceRevision: build.engineRevision, buildId: build.buildId,
        files: { wasm: { sha256: build.wasmSha256 }, worker: { sha256: build.workerSha256 } },
      },
      image: { mimeType: "image/png", width: 1, height: 1, encodedBytes: PNG.length, sha256: hash(PNG) },
    } };
  };
  return {
    openScope: () => scope,
    async open(actualScope, source) {
      assert.equal(actualScope, scope);
      frames = store.openScope({ sessionId, sourceSha256: hash(source), build });
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

async function fixture(t, { sourceName = "scene.py", build = BUILD } = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-feedback-review-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFile(path.join(root, sourceName), SOURCE);
  await runPreviewCli({
    cwd: root, argv: ["--source", sourceName, "--output", "preview", "--time", String(TIME)],
    serviceFactory: () => fakeService(build),
  });
  return { root, sourceName };
}

function review({ root, sourceName }, output = "review-page") {
  return runFeedbackCli({ cwd: root, argv: [
    "review", "--preview", "preview/manifest.json", "--source", sourceName, "--output", output,
  ] });
}

async function pageData(indexPath) {
  const html = await readFile(indexPath, "utf8");
  const script = html.match(/<script\b(?=[^>]*\bid=["']noon-review-data["'])[^>]*>([\s\S]*?)<\/script>/i);
  assert.ok(script, "review page must contain its bound capture data");
  assert.match(script[0], /type=["']application\/json["']/);
  return { html, encoded: script[1], data: JSON.parse(script[1]) };
}

function noteFor(data, sampleIndex = 1) {
  const selected = data.frames[sampleIndex];
  return {
    kind: "noon-frame-feedback-note", schemaVersion: 1,
    sourceSha256: data.source.sha256, sampleIndex: selected.sampleIndex,
    frameIdentity: selected.identity, feedback: structuredClone(INPUT),
  };
}

async function readyReview(t, options) {
  const source = await fixture(t, options);
  const result = await review(source);
  const { data } = await pageData(result.indexPath);
  const note = noteFor(data);
  await writeFile(path.join(source.root, "downloaded-note.json"), JSON.stringify(note));
  return { ...source, result, data, note };
}

function importNote(root, output = "accepted-feedback") {
  return runFeedbackCli({ cwd: root, argv: [
    "import", "--review", "review-page", "--note", "downloaded-note.json", "--output", output,
  ] });
}

async function modifyJson(filename, mutate) {
  const value = JSON.parse(await readFile(filename, "utf8"));
  mutate(value);
  await writeFile(filename, JSON.stringify(value));
}

async function assertMissing(filename) {
  await assert.rejects(access(filename), { code: "ENOENT" });
}

test("review export and bound note import retain exact original evidence and full-precision time", async (t) => {
  const { root, result, data, note } = await readyReview(t);
  assert.equal(result.outputDir, path.join(root, "review-page"));
  assert.equal(result.indexPath, path.join(result.outputDir, "index.html"));
  assert.equal(result.frames, 2);
  assert.equal(data.kind, "noon-frame-review");
  assert.equal(data.schemaVersion, 1);
  assert.deepEqual(data.source, { name: "scene.py", sha256: hash(SOURCE) });
  const original = JSON.parse(await readFile(path.join(root, "preview/manifest.json"), "utf8"));
  for (const [index, selected] of data.frames.entries()) {
    const { retentionMs: _retentionMs, ...frame } = original.samples[index].artifact;
    assert.equal(selected.sampleIndex, index);
    assert.match(selected.identity, /^[a-f0-9]{64}$/);
    assert.equal(selected.filename, `frame-${String(index).padStart(3, "0")}.png`);
    assert.deepEqual(selected.frame, frame);
    assert.deepEqual(await readFile(path.join(result.outputDir, selected.filename)), PNG);
  }
  assert.equal(data.frames[1].frame.provenance.publishedTime, TIME);
  assert.equal(await readFile(path.join(result.outputDir, "source.txt"), "utf8"), SOURCE);
  for (const filename of ["review.js", "review.css"]) {
    assert.ok((await readFile(path.join(result.outputDir, filename))).length > 0);
  }

  // Finalization uses the preserved source, not a later edit at the old path.
  await writeFile(path.join(root, "scene.py"), `${SOURCE}# Subsequent source edit.\n`);
  await modifyJson(path.join(result.outputDir, "manifest.json"), (manifest) => {
    manifest.source.path = "/must/not/read/a/manifest-selected/source.py";
  });
  note.feedback.interval = { start: TIME, end: TIME };
  await writeFile(path.join(root, "downloaded-note.json"), JSON.stringify(note));
  const accepted = await importNote(root);
  assert.equal(accepted.feedback.status, "pending_review");
  assert.deepEqual(accepted.feedback.frame, data.frames[1].frame);
  assert.deepEqual(accepted.feedback.feedback, note.feedback);
  assert.deepEqual(await readFile(path.join(accepted.outputDir, "frame.png")), PNG);
  assert.equal(await readFile(path.join(accepted.outputDir, "source.txt"), "utf8"), SOURCE);
  assert.deepEqual(JSON.parse(await readFile(accepted.feedbackPath, "utf8")), accepted.feedback);
  assert.equal((await runFeedbackCli({ cwd: root, argv: [
    "check", "--feedback", "accepted-feedback/feedback.json", "--source", "scene.py",
  ] })).sourceStatus, "stale");
});

test("review import preserves a clicked point and rejects out-of-image coordinates", async (t) => {
  const { root, data } = await readyReview(t);
  const marked = noteFor(data);
  delete marked.feedback.region;
  marked.feedback.point = { x: 0.42, y: 0.61 };
  marked.feedback.desiredChange = "Propose a focused correction for this comment.";
  await writeFile(path.join(root, "downloaded-note.json"), JSON.stringify(marked));
  const imported = await importNote(root);
  assert.deepEqual(imported.feedback.feedback.point, marked.feedback.point);
  assert.equal(imported.feedback.feedback.region, null);
  assert.equal(imported.feedback.frame.provenance.publishedTime, TIME);
  assert.deepEqual(await readFile(path.join(imported.outputDir, "frame.png")), PNG);
  const invalid = { ...marked, feedback: { ...marked.feedback, point: { x: 1.001, y: 0.61 } } };
  await writeFile(path.join(root, "downloaded-note.json"), JSON.stringify(invalid));
  await assert.rejects(importNote(root, "invalid-point"));
  await assertMissing(path.join(root, "invalid-point"));
});

test("embedded capture data escapes markup while retaining literal source and build text", async (t) => {
  const sourceName = "scene<&\"'>.py";
  const attack = "</script><script>globalThis.feedbackInjection=true</script><img src=x onerror=alert(1)>";
  const source = await fixture(t, { sourceName, build: { ...BUILD, buildId: attack } });
  const result = await review(source);
  const { html, encoded, data } = await pageData(result.indexPath);
  assert.equal(data.source.name, sourceName);
  assert.equal(data.frames[0].frame.provenance.build.buildId, attack);
  assert.doesNotMatch(encoded, /</);
  assert.equal(html.includes(attack), false, "metadata must never become an executable HTML fragment");
  assert.equal(html.includes("__NOON_REVIEW_DATA__"), false);
});

test("import binds the note to its selected source and frame rather than just matching PNG bytes", async (t) => {
  for (const mutation of ["sample", "source", "identity", "extra-status", "invalid-interval"]) {
    await t.test(mutation, async (t) => {
      const { root } = await readyReview(t);
      await modifyJson(path.join(root, "downloaded-note.json"), (note) => {
        if (mutation === "sample") { note.sampleIndex = 0; note.feedback.interval = { start: 0, end: 2 }; }
        if (mutation === "source") note.sourceSha256 = "f".repeat(64);
        if (mutation === "identity") note.frameIdentity = "f".repeat(64);
        if (mutation === "extra-status") note.status = "resolved";
        if (mutation === "invalid-interval") note.feedback.interval = { start: 2, end: 3 };
      });
      await assert.rejects(importNote(root));
      await assertMissing(path.join(root, "accepted-feedback"));
    });
  }
});

test("coherent changes to capture provenance invalidate the previously downloaded note", async (t) => {
  for (const mutation of ["artifact-id", "session", "time", "backend", "build-id", "wasm", "worker", "revision"]) {
    await t.test(mutation, async (t) => {
      const { root } = await readyReview(t);
      const filename = path.join(root, "review-page/manifest.json");
      await modifyJson(filename, (manifest) => {
        const sample = manifest.samples[1];
        if (mutation === "artifact-id") sample.artifact.id = "different-captured-artifact";
        if (mutation === "session") sample.artifact.provenance.sessionId = "different-capture-session";
        if (mutation === "time") {
          sample.artifact.provenance.publishedTime = 1.75;
          sample.snapshot.frame.publishedTime = 1.75;
        }
        if (mutation === "backend") {
          sample.artifact.provenance.backend = "other-fixture";
          sample.snapshot.frame.rendererBackend = "other-fixture";
        }
        if (mutation === "build-id") {
          sample.artifact.provenance.build.buildId = "different-build";
          sample.snapshot.buildIdentity.buildId = "different-build";
        }
        for (const name of ["wasm", "worker"]) {
          if (mutation === name) {
            sample.artifact.provenance.build[`${name}Sha256`] = "e".repeat(64);
            sample.snapshot.buildIdentity.files[name].sha256 = "e".repeat(64);
          }
        }
        if (mutation === "revision") sample.artifact.provenance.sceneRevision = 2;
      });
      const manifest = JSON.parse(await readFile(filename, "utf8"));
      assert.doesNotThrow(() => buildFrameFeedback({ descriptor: manifest.samples[1].artifact, png: PNG }, INPUT),
        "the replacement capture is valid evidence in isolation");
      await assert.rejects(importNote(root), /identity|reference|match|binding/i);
      await assertMissing(path.join(root, "accepted-feedback"));
    });
  }
});

test("review validates every captured frame before publishing and import rechecks saved bytes", async (t) => {
  for (const mutation of ["export-image", "import-image", "import-source", "dimensions"]) {
    await t.test(mutation, async (t) => {
      const source = await fixture(t);
      if (mutation === "export-image") {
        const changed = Buffer.from(PNG); changed[45] ^= 1;
        await writeFile(path.join(source.root, "preview/frame-001.png"), changed);
        await assert.rejects(review(source));
        await assertMissing(path.join(source.root, "review-page"));
        return;
      }
      const result = await review(source);
      const { data } = await pageData(result.indexPath);
      await writeFile(path.join(source.root, "downloaded-note.json"), JSON.stringify(noteFor(data)));
      if (mutation === "import-image") {
        const changed = Buffer.from(PNG); changed[45] ^= 1;
        await writeFile(path.join(result.outputDir, "frame-001.png"), changed);
      } else if (mutation === "import-source") {
        await writeFile(path.join(result.outputDir, "source.txt"), `${SOURCE}# Changed snapshot.\n`);
      } else {
        await modifyJson(path.join(result.outputDir, "manifest.json"), (manifest) => {
          manifest.samples[1].artifact.width = 2;
          manifest.samples[1].snapshot.image.width = 2;
        });
      }
      await assert.rejects(importNote(source.root));
      await assertMissing(path.join(source.root, "accepted-feedback"));
    });
  }
});

test("review import confines its manifest, source snapshot, and PNG to the review folder", async (t) => {
  for (const filename of ["manifest.json", "source.txt", "frame-001.png"]) {
    await t.test(filename, async (t) => {
      const { root, result } = await readyReview(t);
      const saved = path.join(result.outputDir, filename);
      await writeFile(path.join(root, filename), await readFile(saved));
      await rm(saved);
      await symlink(`../${filename}`, saved);
      await assert.rejects(importNote(root), /escapes|confined/i);
      await assertMissing(path.join(root, "accepted-feedback"));
    });
  }
});

test("occupied outputs and invalid inputs leave accepted evidence untouched", async (t) => {
  const source = await fixture(t);
  const occupied = path.join(source.root, "occupied");
  await mkdir(occupied);
  await writeFile(path.join(occupied, "accepted.txt"), "accepted evidence");
  await assert.rejects(review(source, "occupied"), { code: "EEXIST" });
  const result = await review(source);
  const { data } = await pageData(result.indexPath);
  await writeFile(path.join(source.root, "downloaded-note.json"), JSON.stringify(noteFor(data)));
  await assert.rejects(importNote(source.root, "occupied"), { code: "EEXIST" });
  assert.equal(await readFile(path.join(occupied, "accepted.txt"), "utf8"), "accepted evidence");

  await writeFile(path.join(source.root, "downloaded-note.json"), ` ${" ".repeat(256 * 1024)}${JSON.stringify(noteFor(data))}`);
  await assert.rejects(importNote(source.root));
  await assertMissing(path.join(source.root, "accepted-feedback"));
  await writeFile(path.join(source.root, source.sourceName), `${SOURCE}# Changed original.\n`);
  await assert.rejects(review(source, "stale-page"));
  await assertMissing(path.join(source.root, "stale-page"));
});

// Add a valid unknown ancillary chunk without changing the one-pixel image.
// This exercises encoded-byte quotas using a well-framed, CRC-correct PNG.
function paddedPng(size) {
  const payload = Buffer.alloc(size - PNG.length - 12);
  const kind = Buffer.from("npAD");
  const table = Array.from({ length: 256 }, (_, n) => {
    let crc = n;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    return crc >>> 0;
  });
  let crc = 0xffffffff;
  for (const bytes of [kind, payload]) {
    for (const byte of bytes) crc = table[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  const chunk = Buffer.alloc(payload.length + 12);
  chunk.writeUInt32BE(payload.length, 0); kind.copy(chunk, 4); payload.copy(chunk, 8);
  chunk.writeUInt32BE((crc ^ 0xffffffff) >>> 0, chunk.length - 4);
  return Buffer.concat([PNG.subarray(0, -12), chunk, PNG.subarray(-12)]);
}

test("review publication bounds the total sample count and encoded frame bytes", async (t) => {
  for (const bound of ["count", "encoded-bytes"]) {
    await t.test(bound, async (t) => {
      const source = await fixture(t);
      const bytes = bound === "encoded-bytes" ? paddedPng(8 * 1024 * 1024) : PNG;
      if (bound === "encoded-bytes") await writeFile(path.join(source.root, "preview/frame-000.png"), bytes);
      await modifyJson(path.join(source.root, "preview/manifest.json"), (manifest) => {
        const sample = manifest.samples[0];
        sample.artifact.byteLength = bytes.length;
        sample.artifact.sha256 = hash(bytes);
        sample.snapshot.image.encodedBytes = bytes.length;
        sample.snapshot.image.sha256 = sample.artifact.sha256;
        manifest.samples = Array.from({ length: bound === "count" ? 33 : 9 }, (_, index) => {
          const copy = structuredClone(sample);
          copy.artifact.id = `synthetic-bounded-frame-${index}`;
          return copy;
        });
      });
      await assert.rejects(review(source), /limit|budget|32|64|sample|byte/i);
      await assertMissing(path.join(source.root, "review-page"));
    });
  }
});
