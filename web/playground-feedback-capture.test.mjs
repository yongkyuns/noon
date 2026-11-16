import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import {
  assertPausedPublication, capturePausedFeedback, copyDisplayedCanvas,
  createLiveFeedbackExport, feedbackPoint,
} from "./playground-feedback-capture.js";

const PNG = Uint8Array.from(Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgAAIAAAUAAXpeqz8AAAAASUVORK5CYII=", "base64"));
const digest = bytes => createHash("sha256").update(bytes).digest("hex");
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
function fixture() {
  const calls = [];
  const receipt = { session: 7, sequence: 11, time: 1.2345678901234567 };
  const state = { preparedPlayback: true, playing: false, time: receipt.time, presentationReceipt: receipt };
  const control = { current: true, editorSource: "unsubmitted edit", copies: 0 };
  const context = {
    source: "from noon import *\n# accepted Run — not the editor\n", runGeneration: 3,
    canvas: { fixtureOnly: true }, isCurrent: () => control.current,
    async buildIdentity() { calls.push("build"); return { sourceRevision: "a".repeat(40),
      files: { wasm: { sha256: "b".repeat(64) }, worker: { sha256: "c".repeat(64) } }, buildId: "actual-observed-build" }; },
    player: {
      rendererBackend: "synthetic-host-fixture",
      async pause() { calls.push("pause"); return structuredClone(state); },
      async seek(time) { calls.push(["seek", time]); return structuredClone(state); },
      async state() { calls.push("state"); return structuredClone(state); },
    },
  };
  const adapters = {
    async afterPaint() { calls.push("paint"); },
    copyImage(canvas) {
      assert.equal(canvas, context.canvas); calls.push("copy"); control.copies++;
      return { width: 1, height: 1, png: Promise.resolve(PNG) };
    },
    onPaused(observation) { calls.push(["observe", observation.time]); },
  };
  return { context, adapters, control, receipt, state, calls };
}

test("capture pauses and re-presents through the existing player, bracketing the synchronous copy", async () => {
  const f = fixture();
  const capture = await capturePausedFeedback(f.context, f.adapters);
  assert.deepEqual(f.calls, ["pause", ["observe", f.receipt.time], ["seek", f.receipt.time],
    ["observe", f.receipt.time], "build", "paint", "state", "copy", "state"]);
  assert.equal(capture.source, f.context.source);
  assert.notEqual(capture.source, f.control.editorSource);
  assert.equal(capture.frame.provenance.sourceSha256, digest(f.context.source));
  assert.equal(capture.frame.sha256, digest(PNG));
  assert.equal(capture.frame.provenance.requestedTime, f.receipt.time);
  assert.equal(capture.frame.provenance.publishedTime, f.receipt.time);
  assert.equal(capture.frame.provenance.sessionId, "playground:3:7");
  assert.equal(capture.frame.provenance.frameRevision, null);
  assert.equal(capture.frame.provenance.sceneRevision, null);
  assert.deepEqual(capture.presentationReceipt, f.receipt);
  assert.deepEqual(capture.png, PNG);
  assert.notEqual(capture.png, PNG);
  assert.ok(Object.isFrozen(capture.frame.provenance));
  assert.equal(capture.frame.provenance.build.buildId, "actual-observed-build");
});

test("unknown build observations stay null rather than inheriting checkout identity", async () => {
  const f = fixture();
  f.context.buildIdentity = async () => ({ sourceRevision: "not-a-revision", buildId: "\0" });
  const capture = await capturePausedFeedback(f.context, f.adapters);
  assert.deepEqual(capture.frame.provenance.build, {
    engineRevision: null, wasmSha256: null, workerSha256: null, buildId: null,
  });
});

test("invalid publication observations fail closed", () => {
  const f = fixture();
  for (const state of [
    { ...f.state, playing: true }, { ...f.state, preparedPlayback: false },
    { ...f.state, time: Infinity }, { ...f.state, time: 601 },
    { ...f.state, presentationReceipt: undefined },
    { ...f.state, presentationReceipt: { ...f.receipt, session: -1 } },
    { ...f.state, presentationReceipt: { ...f.receipt, sequence: 0.5 } },
    { ...f.state, presentationReceipt: { ...f.receipt, time: f.receipt.time + 1 } },
  ]) assert.throws(() => assertPausedPublication(state));
  assert.equal(assertPausedPublication(f.state).time, f.receipt.time);
});

for (const phase of ["before-copy", "after-copy"]) {
  for (const field of ["session", "sequence", "time"]) {
    test(`changed ${field} ${phase} is rejected rather than attached to the wrong picture`, async () => {
      const f = fixture(); let reads = 0;
      f.context.player.state = async () => {
        reads++;
        const changed = reads === (phase === "before-copy" ? 1 : 2);
        const result = structuredClone(f.state);
        if (changed) {
          result.presentationReceipt[field]++;
          if (field === "time") result.time = result.presentationReceipt.time;
        }
        return result;
      };
      await assert.rejects(capturePausedFeedback(f.context, f.adapters), /changed during/);
      assert.equal(f.control.copies, phase === "before-copy" ? 0 : 1);
    });
  }
}

for (const phase of ["pause", "seek", "build", "paint", "copy", "encode"]) {
  test(`a replaced Run during ${phase} cannot produce a review artifact`, async () => {
    const f = fixture();
    if (phase === "pause" || phase === "seek") {
      f.context.player[phase] = async () => { f.control.current = false; return f.state; };
    } else if (phase === "build") f.context.buildIdentity = async () => { f.control.current = false; return {}; };
    else if (phase === "paint") f.adapters.afterPaint = async () => { f.control.current = false; };
    else if (phase === "copy") f.adapters.copyImage = () => {
      f.control.current = false; return { width: 1, height: 1, png: Promise.resolve(PNG) };
    };
    else f.adapters.copyImage = () => ({ width: 1, height: 1,
      png: Promise.resolve().then(() => { f.control.current = false; return PNG; }) });
    await assert.rejects(capturePausedFeedback(f.context, f.adapters), { name: "AbortError" });
  });
}

test("cancellation before capture has no player side effects", async () => {
  const f = fixture(), controller = new AbortController(); controller.abort();
  await assert.rejects(capturePausedFeedback(f.context, { ...f.adapters, signal: controller.signal }), { name: "AbortError" });
  assert.deepEqual(f.calls, []);
});

test("cancelled pending pause never queues a later seek", async () => {
  const f = fixture(), wait = deferred(), controller = new AbortController();
  f.context.player.pause = () => wait.promise;
  const capture = capturePausedFeedback(f.context, { ...f.adapters, signal: controller.signal });
  controller.abort(); wait.resolve(f.state);
  await assert.rejects(capture, { name: "AbortError" });
  assert.deepEqual(f.calls, []);
});

test("paused time and receipt must agree exactly; an endpoint adjustment is not relabelled", async () => {
  const f = fixture();
  f.context.player.seek = async () => ({ ...f.state, time: 1,
    presentationReceipt: { ...f.receipt, time: 1 } });
  await assert.rejects(capturePausedFeedback(f.context, f.adapters), /requested paused time/);
  assert.equal(f.control.copies, 0);
});

test("preflight rejects missing or oversized source before any pause", async () => {
  for (const source of ["", " ", "a\0b", "\ud800", "a".repeat(1_000_001)]) {
    const f = fixture(); f.context.source = source;
    await assert.rejects(capturePausedFeedback(f.context, f.adapters), /source snapshot/);
    assert.deepEqual(f.calls, []);
  }
});

test("size and encoding failures produce no accepted capture", async () => {
  for (const copied of [
    { width: 0, height: 1, png: PNG }, { width: 8193, height: 1, png: PNG },
    { width: 8192, height: 8192, png: PNG }, { width: 1, height: 1, png: new Uint8Array() },
    { width: 1, height: 1, png: new Uint8Array(8 * 1024 * 1024 + 1) },
    { width: 1, height: 1, png: null },
  ]) {
    const f = fixture(); f.adapters.copyImage = () => copied;
    await assert.rejects(capturePausedFeedback(f.context, f.adapters));
  }
  const f = fixture();
  f.adapters.copyImage = () => ({ width: 1, height: 1, png: Promise.reject(new Error("encoder failed")) });
  await assert.rejects(capturePausedFeedback(f.context, f.adapters), /encoder failed/);
});

test("point mapping excludes letterboxing and preserves positions after resizing", () => {
  const wide = { left: 10, top: 20, width: 400, height: 400 };
  assert.deepEqual(feedbackPoint(210, 220, wide, 800, 400), { x: 0.5, y: 0.5 });
  assert.equal(feedbackPoint(210, 50, wide, 800, 400), null);
  assert.deepEqual(feedbackPoint(10, 120, wide, 800, 400), { x: 0, y: 0 });
  assert.deepEqual(feedbackPoint(410, 320, wide, 800, 400), { x: 1, y: 1 });
  assert.deepEqual(feedbackPoint(110, 220, wide, 800, 400), { x: 0.25, y: 0.5 });
  assert.deepEqual(feedbackPoint(60, 120, { left: 10, top: 20, width: 200, height: 200 }, 800, 400), { x: 0.25, y: 0.5 });
  assert.equal(feedbackPoint(0, 0, { left: 0, top: 0, width: 0, height: 1 }, 1, 1), null);
  assert.equal(feedbackPoint(NaN, 0, wide, 1, 1), null);
});

test("one comment exports exact source, image and original publication without resolving feedback", async () => {
  const f = fixture(), capture = await capturePausedFeedback(f.context, f.adapters);
  const point = { x: 0.25, y: 0.75 };
  const exported = createLiveFeedbackExport(capture, point, "Keep this arrow visible.");
  assert.equal(exported.kind, "noon-live-feedback-export");
  assert.equal(exported.feedback.status, "pending_review");
  assert.deepEqual(Buffer.from(exported.pngBase64, "base64"), Buffer.from(PNG));
  assert.equal(exported.source, f.context.source);
  assert.equal(exported.feedback.frame, capture.frame);
  assert.deepEqual(exported.feedback.feedback.interval, { start: f.receipt.time, end: f.receipt.time });
  assert.deepEqual(exported.feedback.feedback.point, point);
  point.x = 0;
  assert.equal(exported.feedback.feedback.point.x, 0.25);
  assert.equal(exported.feedback.feedback.region, null);
  assert.deepEqual(exported.presentationReceipt, f.receipt);
  for (const comment of ["", "   ", "x\0y", "é".repeat(2001)]) {
    assert.throws(() => createLiveFeedbackExport(capture, point, comment));
  }
  assert.doesNotThrow(() => createLiveFeedbackExport(capture, point, "é".repeat(2000)));
  for (const point of [null, { x: -1, y: 0.5 }, { x: 0.5, y: Infinity }, { x: 0.5, y: 1.01 }]) {
    assert.throws(() => createLiveFeedbackExport(capture, point, "Comment"));
  }
});

test("canvas pixels are copied synchronously, encoding is asynchronous, and temporary storage is released", async () => {
  let pixelsCopied = false, encode;
  const copy = { width: 0, height: 0,
    getContext: () => ({ drawImage: () => { pixelsCopied = true; } }),
    toBlob: callback => { encode = callback; },
  };
  const canvas = { width: 1, height: 1, ownerDocument: { createElement: () => copy } };
  const result = copyDisplayedCanvas(canvas);
  assert.equal(pixelsCopied, true);
  assert.equal(copy.width, 1);
  encode(new Blob([PNG], { type: "image/png" }));
  assert.deepEqual(await result.png, PNG);
  assert.equal(copy.width, 1); assert.equal(copy.height, 1);
});
