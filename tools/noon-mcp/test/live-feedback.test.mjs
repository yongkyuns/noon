import assert from "node:assert/strict";
import { access, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { capturePausedFeedback, createLiveFeedbackExport } from "../../../web/playground-feedback-capture.js";
import { runFeedbackCli } from "../src/feedback-cli.mjs";

// Synthetic PNG and endpoint; this test does not execute Noon or prove playback.
const PNG = Uint8Array.from(Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgAAIAAAUAAXpeqz8AAAAASUVORK5CYII=", "base64"));
const SOURCE = "from noon import *\n# Original source — not a later draft\n";
async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-live-feedback-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const state = { preparedPlayback: true, playing: false, time: 1.2345678901234567,
    presentationReceipt: { session: 7, sequence: 11, time: 1.2345678901234567 } };
  const capture = await capturePausedFeedback({ source: SOURCE, runGeneration: 2, isCurrent: () => true,
    canvas: {}, buildIdentity: async () => null,
    player: { rendererBackend: "fixture", pause: async () => state, seek: async () => state, state: async () => state },
  }, { afterPaint: async () => {}, copyImage: () => ({ width: 1, height: 1, png: PNG }) });
  const exported = createLiveFeedbackExport(capture, {
    region: { x: 0.25, y: 0.5, width: 0.5, height: 0.5 },
  }, "Keep this input visible.");
  await writeFile(path.join(root, "note.json"), JSON.stringify(exported));
  // Imported JSON is mutable external input, not the frozen in-process capture.
  return { root, exported: JSON.parse(JSON.stringify(exported)) };
}
function importNote(root, output = "review") {
  return runFeedbackCli({ cwd: root, argv: ["import-live", "--input", "note.json", "--output", output] });
}

test("live browser export becomes the existing verified pending feedback bundle", async t => {
  const { root, exported } = await fixture(t);
  const imported = await importNote(root);
  assert.deepEqual(imported.feedback, exported.feedback);
  assert.equal(imported.feedback.status, "pending_review");
  assert.deepEqual(await readFile(path.join(root, "review/frame.png")), Buffer.from(PNG));
  assert.equal(await readFile(path.join(root, "review/source.txt"), "utf8"), SOURCE);
  assert.deepEqual(JSON.parse(await readFile(path.join(root, "review/capture.json"), "utf8")).presentationReceipt,
    exported.presentationReceipt);
  await writeFile(path.join(root, "scene.py"), SOURCE);
  const check = () => runFeedbackCli({ cwd: root, argv: ["check", "--feedback", "review/feedback.json", "--source", "scene.py"] });
  assert.equal((await check()).sourceStatus, "current");
  await writeFile(path.join(root, "scene.py"), `${SOURCE}# Edited later\n`);
  assert.equal((await check()).sourceStatus, "stale");
  assert.deepEqual(JSON.parse(await readFile(imported.feedbackPath, "utf8")), exported.feedback);
});

const mutations = {
  source: x => { x.source += "# Wrong Run\n"; },
  png: x => { const bytes = Buffer.from(x.pngBase64, "base64"); bytes[45] ^= 1; x.pngBase64 = bytes.toString("base64"); },
  base64: x => { x.pngBase64 += "\n"; },
  receiptSession: x => { x.presentationReceipt.session++; },
  receiptTime: x => { x.presentationReceipt.time++; },
  invalidSequence: x => { x.presentationReceipt.sequence = -1; },
  resolved: x => { x.feedback.status = "resolved"; },
  region: x => { x.feedback.feedback.region.width = 0.85; },
  ambiguousSelection: x => { x.feedback.feedback.point = { x: 0.5, y: 0.5 }; },
  unknownField: x => { x.sourcePath = "/not/a/file/to/read"; },
  sourceNul: x => { x.source = "a\0b"; },
  sourceUnicode: x => { x.source = "\ud800"; },
  dimensions: x => { x.feedback.frame.width = 2; },
};
for (const [name, change] of Object.entries(mutations)) {
  test(`invalid ${name} never publishes a feedback directory`, async t => {
    const { root, exported } = await fixture(t);
    change(exported);
    await writeFile(path.join(root, "note.json"), JSON.stringify(exported));
    await assert.rejects(importNote(root));
    await assert.rejects(access(path.join(root, "review")), { code: "ENOENT" });
  });
}

test("live import preserves occupied output and rejects oversized input", async t => {
  const { root } = await fixture(t);
  const first = await importNote(root);
  const before = await readFile(first.feedbackPath);
  await assert.rejects(importNote(root), { code: "EEXIST" });
  assert.deepEqual(await readFile(first.feedbackPath), before);
  await writeFile(path.join(root, "note.json"), " ".repeat(18 * 1024 * 1024 + 1));
  await assert.rejects(importNote(root, "oversized"));
  await assert.rejects(access(path.join(root, "oversized")), { code: "ENOENT" });
});
