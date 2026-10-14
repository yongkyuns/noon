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
function fixture() {
  const urls = new Set(), revoked = [], copies = [], decoders = [];
  let nextUrl = 0, failCopy = false, failDecode = false;
  class Element {
    constructor(tag) { this.tag = tag; this.children = []; this.dataset = {}; this.ownerDocument = document; }
    append(...nodes) { for (const node of nodes) { node.parent = this; this.children.push(node); } }
    prepend(node) { node.parent = this; this.children.unshift(node); }
    setAttribute(name, value) { this[name] = value; }
    removeAttribute(name) { delete this[name]; }
    remove() { if (this.parent) this.parent.children = this.parent.children.filter(x => x !== this); }
    decode() { if (failDecode) return Promise.reject(new Error("decode failed")); return Promise.resolve(); }
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
  return { preview, frame, canvas, overlay, copies, urls, revoked, decoders,
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
test("rapid successful replacements do not build an unbounded capture queue", async () => {
  const f = fixture();
  for (let i = 0; i < 20; i++) { f.preview.accept(f.canvas, identity(i)); f.preview.hold(f.canvas); }
  assert.equal(f.copies.length, 1); assert.equal(f.decoders.length, 1);
  f.encode(); await flush(); assert.equal(f.urls.size, 0);
});
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
test("clearing during image decode revokes the pending URL", async () => {
  const f = fixture(); const decode = deferred();
  const create = f.frame.ownerDocument.createElement;
  f.frame.ownerDocument.createElement = tag => { const element = create(tag); if (tag === "img") element.decode = () => decode.promise; return element; };
  f.preview.accept(f.canvas, identity()); f.preview.hold(f.canvas); f.encode(); await flush();
  assert.equal(f.urls.size, 1); f.preview.clear(); assert.equal(f.urls.size, 0);
  decode.resolve(); await flush(); assert.equal(f.overlay.hidden, true);
});

test("actual edit boundary freezes pixels before hiding or retiring the renderer", async () => {
  const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
  const start = main.indexOf("let editStopPromise = null;");
  const end = main.indexOf("function sameSemanticContinuation", start);
  const code = main.slice(start, end);
  const order = [], canvas = { style: {} };
  const retained = { pause() { order.push("pause"); return Promise.resolve(); } };
  const context = { canvas, player: retained, playerNeedsRestart: false, sceneRunPromise: null,
    activeSourceContinuation: null, authoringClient: null, runtimePreparation: null,
    previousPreview: { hold(source) { assert.equal(source, canvas); assert.equal(source.style.visibility, undefined); order.push("snapshot"); } },
    playbackControls: { setControllable(value) { assert.equal(value, false); }, setBusy() {} },
    generations: { invalidateRun() {} }, stopMetricsPolling() {}, status: { dataset: {} },
    diagnosticPanel: { invalidate() {} }, setRuntimeStatus() {}, setTimeout, clearTimeout,
    discardEarlyContinuationRuntime() { assert.fail("completed renderer should retain its warm owner"); } };
  await vm.runInNewContext(code + "\nstopForSourceEdit()", context);
  assert.deepEqual(order, ["snapshot", "pause"]);
  assert.equal(canvas.style.visibility, "hidden");
  assert.equal(context.status.dataset.playbackPlaying, "false");
});
