import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import {
  appendSourceFeedback, assertPausedPublication, capturePausedFeedback, copyDisplayedCanvas,
  feedbackPoint, feedbackRegion,
} from "./playground-feedback-capture.js";

const PNG = new Blob([Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgAAIAAAUAAXpeqz8AAAAASUVORK5CYII=", "base64")],
  { type: "image/png" });
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
  const control = { current: true, copies: 0 };
  const context = {
    source: "from noon import *\n# accepted Run — not the editor\n",
    canvas: { fixtureOnly: true }, isCurrent: () => control.current,
    player: {
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
    ["observe", f.receipt.time], "paint", "state", "copy", "state"]);
  assert.equal(capture.sourceSha256, digest(f.context.source));
  assert.equal(capture.width, 1);
  assert.equal(capture.height, 1);
  assert.deepEqual(capture.presentationReceipt, f.receipt);
  assert.equal(capture.png, PNG);
});

test("finite soundtrack feedback reads the replay state before pausing and uses its matched frame", async () => {
  const f = fixture();
  f.context.checkReplay = true;
  Object.assign(f.state, { preparedPlayback: false, hasAudio: true, replaySupported: false });
  await assert.rejects(capturePausedFeedback(f.context, f.adapters), /prepared or finite soundtrack playback/);
  assert.deepEqual(f.calls, ["state"]);
  f.state.replaySupported = true;
  const capture = await capturePausedFeedback(f.context, f.adapters);
  assert.deepEqual(capture.presentationReceipt, f.receipt);
  assert.equal(f.control.copies, 1);
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

test("object hints require a renderer view that stayed current through the image copy", async () => {
  const f = fixture();
  f.receipt.viewRevision = 2;
  assert.equal((await capturePausedFeedback(f.context, f.adapters)).viewRevision, 2);
  const copy = f.adapters.copyImage;
  f.adapters.copyImage = canvas => {
    f.receipt.viewRevision = 3;
    return copy(canvas);
  };
  assert.equal((await capturePausedFeedback(f.context, f.adapters)).viewRevision, null);
});

test("capture re-presents the same paused time once if the surface has no receipt yet", async () => {
  const f = fixture();
  let reads = 0;
  const state = f.context.player.state;
  f.context.player.state = async () => {
    if (++reads === 1) {
      f.receipt.sequence++;
      return { ...f.state, presentationReceipt: undefined };
    }
    return state();
  };
  const capture = await capturePausedFeedback(f.context, f.adapters);
  assert.deepEqual(f.calls.filter(call => call[0] === "seek"), [["seek", f.receipt.time], ["seek", f.receipt.time]]);
  assert.equal(capture.presentationReceipt.sequence, f.receipt.sequence);
  assert.equal(f.control.copies, 1);
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

for (const phase of ["pause", "seek", "paint", "copy", "encode"]) {
  test(`a replaced Run during ${phase} cannot supply a comment capture`, async () => {
    const f = fixture();
    if (phase === "pause" || phase === "seek") {
      f.context.player[phase] = async () => { f.control.current = false; return f.state; };
    } else if (phase === "paint") f.adapters.afterPaint = async () => { f.control.current = false; };
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
    { width: 8192, height: 8192, png: PNG }, { width: 1, height: 1, png: PNG.slice(0, 0, "image/png") },
    { width: 1, height: 1, png: new Blob([new Uint8Array(8 * 1024 * 1024 + 1)], { type: "image/png" }) },
    { width: 1, height: 1, png: PNG.slice() },
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

test("a point comment validates its bounds and text before appending to source", async () => {
  const f = fixture(), capture = await capturePausedFeedback(f.context, f.adapters);
  const point = { x: 0.25, y: 0.75 };
  const append = (selection, comment) => appendSourceFeedback(f.context.source, capture, selection, comment);
  assert.match(append({ point }, "Keep this arrow visible."), /# Point on the frame \(normalized\) x=0\.25 y=0\.75/);
  for (const comment of ["", "   ", "x\0y", "é".repeat(2001)]) {
    assert.throws(() => append({ point }, comment));
  }
  assert.doesNotThrow(() => append({ point }, "é".repeat(2000)));
  for (const point of [null, { x: -1, y: 0.5 }, { x: 0.5, y: Infinity }, { x: 0.5, y: 1.01 }]) {
    assert.throws(() => append({ point }, "Comment"));
  }
});

test("reverse drags clamp to the image and write the rectangle to source", async () => {
  const box = { left: 10, top: 20, width: 400, height: 400 };
  const start = feedbackPoint(310, 270, box, 800, 400);
  const end = feedbackPoint(-20, 40, box, 800, 400, true);
  const region = feedbackRegion(start, end);
  assert.deepEqual(region, { x: 0, y: 0, width: 0.75, height: 0.75 });
  assert.equal(feedbackRegion(start, { x: start.x, y: 0 }), null);
  const f = fixture(), capture = await capturePausedFeedback(f.context, f.adapters);
  const append = selection => appendSourceFeedback(f.context.source, capture, selection, "Clarify the highlighted labels.");
  assert.match(append({ region }), /# Rectangle on the frame \(normalized\) x=0 y=0 width=0\.75 height=0\.75/);
  assert.throws(() => append({ region: { ...region, width: 1.1 } }));
  assert.throws(() => append({ region, point: start }));
});

test("source feedback preserves the draft and writes each line as an inert Python comment", async () => {
  const f = fixture(), capture = await capturePausedFeedback(f.context, f.adapters);
  const source = "# Unapplied editor draft\nvalue = 2\n";
  const region = { x: 0.2, y: 0.3, width: 0.4, height: 0.1 };
  const result = appendSourceFeedback(source, capture, { region }, "Keep the arrow.\r\nvalue = 9\r# more\u2028Last line");
  assert.ok(result.startsWith(source));
  assert.deepEqual(result.slice(source.length).trim().split("\n"), [
    `# Noon feedback at ${f.receipt.time} s`,
    "# Rectangle on the frame (normalized) x=0.2 y=0.3 width=0.4 height=0.1",
    `# Displayed source SHA-256 ${digest(f.context.source)}`,
    "# Keep the arrow.", "# value = 9", "# # more", "# Last line",
  ]);
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
  encode(PNG);
  assert.equal(await result.png, PNG);
  assert.equal(copy.width, 1); assert.equal(copy.height, 1);
});
