import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";
import { createPlaygroundPreview } from "./playground-preview.js";

function deferred() {
  let resolve, reject;
  const promise = new Promise((a, b) => { resolve = a; reject = b; });
  return { promise, resolve, reject };
}
const flush = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
const identity = (runGeneration = 1) => ({ exampleId: "square", runGeneration });
function fixture({ decodePromise = null } = {}) {
  const urls = new Set(), revoked = [], copies = [], decoders = [], images = [];
  let nextUrl = 0, failCopy = false, failDecode = false;
  class Element {
    constructor(tag) { this.tag = tag; this.children = []; this.dataset = {}; this.ownerDocument = document; if (tag === "img") images.push(this); }
    append(...nodes) { for (const node of nodes) { node.parent = this; this.children.push(node); } }
    prepend(node) { node.parent = this; this.children.unshift(node); }
    setAttribute(name, value) { this[name] = value; }
    removeAttribute(name) { delete this[name]; }
    remove() { if (this.parent) this.parent.children = this.parent.children.filter(x => x !== this); }
    decode() { if (failDecode) return Promise.reject(new Error("decode failed")); return decodePromise ?? Promise.resolve(); }
    getContext(type) {
      assert.equal(type, "2d");
      return { drawImage: (canvas, ...args) => {
        if (failCopy) throw new Error("capture failed");
        copies.push({ canvas, args, copy: this });
      } };
    }
    toBlob(callback, type) { assert.equal(type, "image/png"); decoders.push(callback); }
  }
  const document = {
    defaultView: { URL: { createObjectURL: () => { const url = `blob:${++nextUrl}`; urls.add(url); return url; },
      revokeObjectURL: url => { revoked.push(url); urls.delete(url); } } },
    createElement: tag => new Element(tag),
  };
  document.head = new Element("head");
  const frame = new Element("div");
  const preview = createPlaygroundPreview(frame);
  const canvas = { width: 1280, height: 720 };
  const overlay = frame.children[0];
  return { preview, frame, canvas, overlay, copies, urls, revoked, decoders, images,
    encode: (value = { size: 200 }) => decoders.shift()(value),
    failCopy() { failCopy = true; }, failDecode() { failDecode = true; } };
}

test("no previous successful run means no fabricated preview", () => {
  const f = fixture(); f.preview.hold(f.canvas);
  assert.equal(f.overlay.hidden, true); assert.equal(f.copies.length, 0);
});
test("capture occurs synchronously once on an edit, not during playback", async () => {
  const f = fixture(); f.preview.accept(f.canvas, identity());
  assert.equal(f.copies.length, 0);
  f.preview.hold(f.canvas); f.preview.hold(f.canvas);
  assert.equal(f.copies.length, 1);
  assert.equal(f.overlay.hidden, true); // Do not show an undecoded/blank image.
  f.encode(); await flush();
  assert.equal(f.overlay.hidden, false);
  assert.equal(f.overlay.dataset.runGeneration, "1");
  assert.match(f.overlay.children.at(-1).textContent, /Previous successful preview.*Edits not applied/);
  assert.equal(f.copies[0].copy.width, 1); // Temporary backing pixels released.
});
test("a failed replacement cannot replace the successful image", async () => {
  const f = fixture(); f.preview.accept(f.canvas, identity()); f.preview.hold(f.canvas);
  f.encode(); await flush(); const image = f.overlay.children[0];
  f.preview.showLive(); assert.equal(f.overlay.hidden, true);
  f.preview.hold(f.canvas, "Edited source failed");
  assert.equal(f.overlay.hidden, false); assert.equal(f.overlay.children[0], image);
  assert.equal(f.copies.length, 1);
});
test("corrected completion hides the stale preview and next edit gets a new identity", async () => {
  const f = fixture(); f.preview.accept(f.canvas, identity()); f.preview.hold(f.canvas);
  f.encode(); await flush(); f.preview.accept(f.canvas, identity(2));
  assert.equal(f.overlay.hidden, true);
  f.preview.hold(f.canvas); f.encode(); await flush();
  assert.equal(f.overlay.dataset.runGeneration, "2");
  assert.equal(f.urls.size, 1); assert.equal(f.revoked.length, 1);
});
test("old encodes cannot arrive over a newer successful run", async () => {
  const f = fixture(); f.preview.accept(f.canvas, identity()); f.preview.hold(f.canvas);
  f.preview.accept(f.canvas, identity(2)); f.encode(); await flush();
  assert.equal(f.overlay.hidden, true); assert.equal(f.urls.size, 0);
  f.preview.hold(f.canvas); f.encode(); await flush();
  assert.equal(f.overlay.dataset.runGeneration, "2");
});
for (const phase of ["encode", "decode"]) {
  test(`rapid replacements during ${phase} keep one capture and allow later recovery`, async () => {
    const decode = deferred();
    const f = fixture({ decodePromise: phase === "decode" ? decode.promise : null });
    f.preview.accept(f.canvas, identity(0)); f.preview.hold(f.canvas);
    if (phase === "decode") { f.encode(); await flush(); }
    for (let i = 1; i <= 20; i++) { f.preview.accept(f.canvas, identity(i)); f.preview.hold(f.canvas); }
    assert.equal(f.copies.length, 1);
    assert.equal(f.decoders.length, phase === "encode" ? 1 : 0);
    assert.equal(f.urls.size, phase === "encode" ? 0 : 1);
    if (phase === "encode") f.encode(); else decode.resolve();
    await flush(); assert.equal(f.urls.size, 0); assert.equal(f.overlay.hidden, true);
    f.preview.accept(f.canvas, identity(21)); f.preview.hold(f.canvas);
    assert.equal(f.copies.length, 2); f.encode(); await flush();
    assert.equal(f.overlay.dataset.runGeneration, "21"); assert.equal(f.urls.size, 1);
    f.preview.clear(); assert.equal(f.urls.size, 0);
  });
}
test("selection clear prevents delayed pixels from the previous example", async () => {
  const f = fixture(); f.preview.accept(f.canvas, identity()); f.preview.hold(f.canvas);
  f.preview.clear(); f.encode(); await flush();
  assert.equal(f.overlay.hidden, true); assert.equal(f.urls.size, 0);
});
test("retired or replaced canvas is not silently attributed to the accepted run", () => {
  const f = fixture(); f.preview.accept(f.canvas, identity()); f.preview.hold({ ...f.canvas });
  assert.equal(f.copies.length, 0); assert.equal(f.overlay.hidden, true);
});
for (const failure of ["copy", "decode", "empty", "oversize"]) {
  test(`optional ${failure} failure leaves the view usable`, async () => {
    const f = fixture(); f.preview.accept(f.canvas, identity());
    if (failure === "copy") f.failCopy();
    if (failure === "decode") f.failDecode();
    f.preview.hold(f.canvas);
    if (failure !== "copy") f.encode(failure === "empty" ? null : { size: failure === "oversize" ? 10 * 1024 * 1024 : 123 });
    await flush(); assert.equal(f.overlay.hidden, true); assert.equal(f.urls.size, 0);
    assert.equal(f.copies[0]?.copy.width ?? 1, 1);
  });
}
test("stale image remains while replacement capture is unavailable", async () => {
  const f = fixture(); f.preview.accept(f.canvas, identity()); f.preview.hold(f.canvas);
  f.encode(); await flush(); const prior = f.overlay.children[0];
  f.preview.accept(f.canvas, identity(2)); f.failCopy(); f.preview.hold(f.canvas); await flush();
  assert.equal(f.overlay.children[0], prior); assert.equal(f.overlay.dataset.runGeneration, "1");
});
test("capture has bounded pixel dimensions without changing the source canvas", async () => {
  const f = fixture(); f.canvas.width = 7680; f.canvas.height = 4320;
  f.preview.accept(f.canvas, identity()); f.preview.hold(f.canvas);
  const [, , width, height] = f.copies[0].args;
  assert.ok(width <= 1536 && height <= 1536 && width * height <= 1048576);
  assert.ok(Math.abs(width / height - 16 / 9) < 0.01);
  assert.equal(f.canvas.width, 7680); assert.equal(f.canvas.height, 4320);
  f.encode(); await flush();
});
test("dispose frees decoded and pending resources and cannot be resurrected", async () => {
  const f = fixture(); f.preview.accept(f.canvas, identity()); f.preview.hold(f.canvas);
  f.encode(); await flush(); f.preview.dispose();
  assert.equal(f.urls.size, 0); assert.equal(f.frame.children.length, 0);
  f.preview.accept(f.canvas, identity(2)); f.preview.hold(f.canvas);
  assert.equal(f.copies.length, 1);
});
for (const action of ["clear", "accept", "dispose"]) {
  for (const outcome of ["resolve", "reject"]) {
    test(`${action} during ${outcome} image decode retains its URL until settlement`, async () => {
      const decode = deferred(); const f = fixture({ decodePromise: decode.promise });
      f.preview.accept(f.canvas, identity()); f.preview.hold(f.canvas);
      f.encode(); await flush();
      const image = f.images[0];
      const url = image.src;
      if (action === "accept") f.preview.accept(f.canvas, identity(2));
      else f.preview[action]();
      assert.equal(f.overlay.hidden, true);
      assert.equal(f.urls.has(url), true, "an active decoder still owns the blob URL");
      assert.equal(image.src, url, "cancellation must not abort the active image request");
      assert.deepEqual(f.revoked, []);
      f.preview.clear(); f.preview.clear(); f.preview.hold(f.canvas);
      assert.equal(f.copies.length, 1, "cancellation must not queue another capture");
      decode[outcome](new Error("optional decode failure")); await flush();
      assert.equal(f.overlay.hidden, true, "cancelled pixels must never become visible");
      assert.equal(f.urls.size, 0);
      assert.deepEqual(f.revoked, [url], "settlement releases the URL exactly once");
      assert.equal(image.src, undefined);
      assert.equal(f.copies[0].copy.width, 1);
    });
  }
}

// Execute the real profile/retirement helpers with the edit boundary. A stubbed
// profile predicate would hide the difference between an ordinary warm renderer
// and a prepared reader that must be retired before another source is authored.
async function editBoundaryFixture({ prepared = false, pause = Promise.resolve() } = {}) {
  const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
  const section = (first, next) => {
    const start = main.indexOf(first);
    const end = main.indexOf(next, start + first.length);
    assert.ok(start >= 0 && end > start, `missing production section: ${first}`);
    return main.slice(start, end);
  };
  const code = [
    section("function isPreparedPlayback(", "function playbackPresentation("),
    section("function adoptRuntimeCanvas(", "function createRuntimeClient("),
    section("function discardEarlyContinuationRuntime(", "function sameSemanticContinuation("),
  ].join("\n");
  const f = fixture(), order = [];
  const canvas = f.canvas;
  canvas.style = {};
  f.preview.accept(canvas, identity());
  const replacementCanvas = { style: {} };
  let terminated = 0, authoringTerminated = 0, pollsStopped = 0;
  const retained = {
    canvas,
    pause() {
      assert.equal(f.copies.length, 1, "freeze pixels before pausing");
      assert.equal(canvas.style.visibility, "hidden");
      order.push("pause");
      return pause;
    },
    terminate() {
      assert.equal(f.copies.length, 1, "freeze pixels before retiring");
      terminated += 1;
      order.push("terminate");
      this.canvas = replacementCanvas;
    },
  };
  const client = { terminate() { authoringTerminated += 1; } };
  const controls = {
    setControllable(value) { assert.equal(value, false); },
    setBusy(value) { assert.equal(value, true); },
    destroy() { order.push("destroy-controls"); },
  };
  const context = {
    canvas, player: retained, preparedPlaybackPlayer: prepared ? retained : null,
    playerNeedsRestart: false, sceneRunPromise: null, activeSourceContinuation: null,
    authoringClient: client, runtimePreparation: null, playbackControls: controls,
    previousPreview: { hold(source) {
      assert.equal(source, canvas);
      assert.equal(source.style.visibility, undefined, "freeze before hiding");
      assert.equal(terminated, 0, "freeze before renderer retirement");
      order.push("snapshot");
      f.preview.hold(source);
    } },
    generations: { invalidateRun() {} },
    stopMetricsPolling() { pollsStopped += 1; },
    updatePreviewRecoveryControl() {},
    status: { dataset: {} }, diagnosticPanel: { invalidate() {} }, setRuntimeStatus() {},
    setTimeout() { assert.fail("a completed source needs no cancellation watchdog"); },
    clearTimeout() { assert.fail("no watchdog should be allocated"); },
  };
  const stop = vm.runInNewContext(`${code}\n(() => stopForSourceEdit())`, context);
  return { ...f, context, order, retained, client, controls, replacementCanvas, stop,
    counts: () => ({ terminated, authoringTerminated, pollsStopped }) };
}

function assertStopped(context) {
  assert.equal(context.status.dataset.playbackControls, "unavailable");
  assert.equal(context.status.dataset.playbackPhase, "stopped");
  assert.equal(context.status.dataset.playbackPlaying, "false");
}

for (const prepared of [false, true]) test(`${prepared ? "prepared" : "ordinary"} edit freezes pixels before pausing and retains the renderer`, async () => {
  const f = await editBoundaryFixture({ prepared });
  await f.stop();
  assert.deepEqual(f.order, ["snapshot", "pause"]);
  assert.equal(f.canvas.style.visibility, "hidden");
  assert.equal(f.context.player, f.retained);
  assert.equal(f.context.canvas, f.canvas);
  assert.equal(f.context.authoringClient, f.client);
  assert.deepEqual(f.counts(), { terminated: 0, authoringTerminated: 0, pollsStopped: 1 });
  assertStopped(f.context);
  assert.equal(f.overlay.hidden, true, "an unfinished optional encode must not block pausing");
  f.encode(); await flush();
  assert.equal(f.overlay.hidden, false);
  assert.equal(f.overlay.dataset.runGeneration, "1");
});

test("another reader's prepared identity cannot retire the ordinary active renderer", async () => {
  const f = await editBoundaryFixture();
  const foreign = {};
  f.context.preparedPlaybackPlayer = foreign;
  await f.stop();
  assert.deepEqual(f.order, ["snapshot", "pause"]);
  assert.equal(f.context.player, f.retained);
  assert.equal(f.context.preparedPlaybackPlayer, foreign);
  assert.equal(f.counts().terminated, 0);
});

test("a failed completed renderer is frozen and retired rather than paused", async () => {
  const f = await editBoundaryFixture();
  f.context.playerNeedsRestart = true;
  await f.stop();
  assert.deepEqual(f.order, ["snapshot", "terminate", "destroy-controls"]);
  assert.equal(f.context.player, null);
  assert.equal(f.context.playerNeedsRestart, false);
  assert.equal(f.context.authoringClient, f.client);
  assert.equal(f.counts().authoringTerminated, 0);
});

test("repeated edits share the actual pending pause without recapturing pixels", async () => {
  const pause = deferred();
  const f = await editBoundaryFixture({ pause: pause.promise });
  const first = f.stop(), second = f.stop();
  assert.equal(second, first);
  assert.deepEqual(f.order, ["snapshot", "pause"]);
  assert.equal(f.copies.length, 1);
  assertStopped(f.context);
  let settled = false;
  void first.then(() => { settled = true; });
  await flush();
  assert.equal(settled, false, "edit stop must await the real pause acknowledgement");
  pause.resolve(); await first;
  assert.equal(settled, true);
  assert.equal(f.context.player, f.retained);
});

for (const replaced of [false, true]) {
  test(`late pause rejection retires only the original renderer (replaced=${replaced})`, async () => {
    const pause = deferred();
    const f = await editBoundaryFixture({ pause: pause.promise });
    const stopping = f.stop();
    const successor = { canvas: { style: {} } }, successorControls = {};
    if (replaced) {
      f.context.player = successor;
      f.context.canvas = successor.canvas;
      f.context.playbackControls = successorControls;
      f.context.preparedPlaybackPlayer = successor;
    }
    pause.reject(new Error("original pause failed"));
    await stopping;
    assert.deepEqual(f.order, replaced
      ? ["snapshot", "pause", "terminate"]
      : ["snapshot", "pause", "terminate", "destroy-controls"]);
    assert.equal(f.context.player, replaced ? successor : null);
    assert.equal(f.context.canvas, replaced ? successor.canvas : f.replacementCanvas);
    assert.equal(f.context.playbackControls, replaced ? successorControls : null);
    assert.equal(f.context.preparedPlaybackPlayer, replaced ? successor : null);
    assert.equal(f.context.authoringClient, f.client);
    assert.deepEqual(f.counts(), { terminated: 1, authoringTerminated: 0, pollsStopped: replaced ? 1 : 2 });
  });
}
