import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import vm from "node:vm";
import { createDocumentFrameSource, createWorkerFrameSource } from "./render-frame-source.js";

const message = (type, frameId) => ({ channel: "noon.render", protocolVersion: 1, type, frameId });

function fixture() {
  const toDocument = [], toWorker = [], native = new Map(), cancelled = [];
  let nativeId = 0, time = 41, timeReads = 0;
  const owner = createDocumentFrameSource({
    request(callback) { const id = ++nativeId; native.set(id, callback); return id; },
    cancel(id) { cancelled.push(id); native.delete(id); },
    send: (value) => toWorker.push(value),
  });
  const worker = createWorkerFrameSource({
    send: (value) => toDocument.push(value),
    now() { timeReads += 1; return time; },
  });
  return { owner, worker, toDocument, toWorker, native, cancelled,
    get timeReads() { return timeReads; },
    setTime(value) { time = value; },
    deliverRequests() { while (toDocument.length) owner.receive(toDocument.shift()); },
    deliverPulses() { while (toWorker.length) worker.receive(toWorker.shift()); },
    fire() {
      const callbacks = [...native.values()];
      native.clear();
      for (const callback of callbacks) callback(123456);
    },
  };
}

test("idle owns no RAF and a requested pulse uses only the worker clock", () => {
  const f = fixture(), observed = [];
  assert.equal(f.native.size, 0);
  f.worker.request((time) => observed.push(time));
  assert.equal(f.toDocument.length, 1);
  f.deliverRequests();
  assert.equal(f.native.size, 1);
  f.fire();
  assert.deepEqual(Object.keys(f.toWorker[0]).sort(), ["channel", "frameId", "protocolVersion", "type"]);
  f.deliverPulses();
  assert.deepEqual(observed, [41]);
  assert.equal(f.timeReads, 1);
  assert.equal(f.native.size, 0);
  assert.equal(f.toDocument.length, 0);
});

test("one platform opportunity batches callbacks and preserves in-frame cancellation", () => {
  const f = fixture(), observed = [];
  let second;
  f.worker.request(() => {
    observed.push("first");
    f.worker.cancel(second);
    f.worker.request(() => observed.push("next"));
  });
  second = f.worker.request(() => observed.push("cancelled"));
  assert.equal(f.toDocument.length, 1);
  f.deliverRequests(); f.fire(); f.deliverPulses();
  assert.deepEqual(observed, ["first"]);
  assert.equal(f.toDocument.length, 1);
  f.deliverRequests(); f.fire(); f.deliverPulses();
  assert.deepEqual(observed, ["first", "next"]);
});

test("cancelling the final callback retires its one pending document RAF", () => {
  const f = fixture();
  const first = f.worker.request(() => assert.fail("cancelled callback fired"));
  const second = f.worker.request(() => assert.fail("cancelled callback fired"));
  f.deliverRequests();
  const queued = [...f.native.values()][0];
  f.worker.cancel(first);
  assert.equal(f.toDocument.length, 0);
  f.worker.cancel(second);
  f.deliverRequests();
  assert.equal(f.native.size, 0);
  queued();
  assert.equal(f.toWorker.length, 0, "an already queued native callback cannot revive cancellation");
});

test("stale pulses cannot fire a replacement callback", () => {
  const f = fixture(), seen = [];
  const old = f.worker.request(() => assert.fail("retired callback"));
  f.worker.cancel(old);
  f.worker.request(() => seen.push("current"));
  f.worker.receive(message("animation_frame", old));
  assert.deepEqual(seen, []);
  assert.equal(f.timeReads, 0);
  f.deliverRequests(); f.fire(); f.deliverPulses();
  assert.deepEqual(seen, ["current"]);
});

test("callback capacity is bounded without adding extra native requests", () => {
  const f = fixture(), handles = [];
  for (let index = 0; index < 64; index += 1) handles.push(f.worker.request(() => {}));
  assert.throws(() => f.worker.request(() => {}), /callback limit/);
  assert.equal(f.toDocument.length, 1);
  for (const handle of handles) f.worker.cancel(handle);
  f.deliverRequests();
  assert.equal(f.native.size, 0);
  f.worker.request(() => {});
  assert.equal(f.toDocument.length, 1, "capacity is reclaimed after cancellation");
  f.worker.close(); f.deliverRequests();
  assert.equal(f.native.size, 0);
});

test("close retires handles and ignores both queued and later frame deliveries", () => {
  const f = fixture();
  f.worker.request(() => assert.fail("closed callback fired"));
  f.deliverRequests();
  const queued = [...f.native.values()][0];
  f.owner.close();
  queued();
  assert.equal(f.native.size, 0);
  assert.equal(f.toWorker.length, 0);
  f.worker.close();
  f.worker.receive(message("animation_frame", 1));
  assert.throws(() => f.worker.request(() => {}), /closed/);
  assert.equal(f.owner.receive(message("animation_frame_request", 2)), true);
  assert.equal(f.native.size, 0);
});

test("a throwing callback does not suppress other callbacks in the same opportunity", () => {
  const f = fixture(), observed = [];
  f.worker.request(() => { throw new Error("callback failure"); });
  f.worker.request(() => observed.push("second"));
  f.deliverRequests(); f.fire();
  assert.throws(() => f.deliverPulses(), /callback failure/);
  assert.deepEqual(observed, ["second"]);
  f.worker.request(() => observed.push("later"));
  f.deliverRequests(); f.fire(); f.deliverPulses();
  assert.deepEqual(observed, ["second", "later"]);
});

test("invalid and overlapping platform requests fail without a second pending RAF", () => {
  const f = fixture();
  for (const id of [null, "1", 0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
    assert.throws(() => f.owner.receive(message("animation_frame_request", id)), /invalid/);
    assert.throws(() => f.worker.receive(message("animation_frame", id)), /invalid/);
  }
  assert.equal(f.owner.receive(message("metrics", 1)), false);
  assert.equal(f.worker.receive(message(null, 1)), false);
  assert.equal(f.worker.receive({ ...message("animation_frame", 1), channel: "noon.engine" }), false);
  f.owner.receive(message("animation_frame_request", 1));
  assert.throws(() => f.owner.receive(message("animation_frame_request", 2)), /already pending/);
  assert.equal(f.native.size, 1);
  f.owner.close();
});

test("a callback throwing null is still reported after the other callbacks run", () => {
  const f = fixture(), observed = [];
  f.worker.request(() => { throw null; });
  f.worker.request(() => observed.push("second"));
  f.deliverRequests(); f.fire();
  let caught = false;
  try { f.deliverPulses(); } catch (error) { caught = true; assert.equal(error, null); }
  assert.equal(caught, true);
  assert.deepEqual(observed, ["second"]);
});

test("a failed transport send rolls back the pending callback reservation", () => {
  const sent = [];
  let fail = true;
  const worker = createWorkerFrameSource({
    now: () => 3,
    send(value) { if (fail) throw new Error("send failure"); sent.push(value); },
  });
  assert.throws(() => worker.request(() => assert.fail("failed send fired")), /send failure/);
  fail = false;
  const observed = [];
  const handle = worker.request((time) => observed.push(time));
  worker.receive(message("animation_frame", handle));
  assert.deepEqual(observed, [3]);
  worker.close();
});

// Exercise the actual worker entry point as well as the transport helpers.
// Only the renderer factory is replaced, so this does not require a GPU.
const workerEntry = (await readFile(new URL("./authoring-render-worker.js", import.meta.url), "utf8"))
  .replace(/^import .*;\n/gm, "");

function entryFixture(nativeFrames = true) {
  let listener, host;
  const sent = [], dispatched = [];
  const native = () => 77;
  const self = {
    addEventListener(type, callback) { assert.equal(type, "message"); listener = callback; },
    postMessage(message) { sent.push(message); },
    close() { self.closed = true; },
    ...(nativeFrames ? { requestAnimationFrame: native, cancelAnimationFrame() {} } : {}),
  };
  vm.runInNewContext(workerEntry, {
    self, performance: { now: () => 29 }, createWorkerFrameSource,
    createAuthoringRenderController(value) {
      host = value;
      return { dispatch(message) { dispatched.push(message); } };
    },
  });
  return { sent, dispatched, get host() { return host; }, self,
    message(data) { listener({ data }); } };
}

test("render worker binds the negotiated document provider before dispatching prepare", () => {
  const f = entryFixture(), seen = [];
  assert.equal(f.host, undefined);
  f.message({ type: "prepare", documentAnimationFrames: true });
  assert.equal(f.host.animationFrameSource, "document");
  assert.equal(f.dispatched.length, 1);
  const id = f.host.requestAnimationFrame(time => seen.push(time));
  assert.deepEqual(f.sent, [message("animation_frame_request", id)]);
  f.message(message("animation_frame", id));
  assert.deepEqual(seen, [29]);
  assert.equal(f.dispatched.length, 1, "frame pulse must not enter renderer command dispatch");
  f.host.requestAnimationFrame(() => assert.fail("closed provider callback"));
  f.host.close();
  assert.equal(f.self.closed, true);
  assert.equal(f.sent.at(-1).type, "animation_frame_cancel");
});

test("standalone worker hosts keep their native RAF or timer capability", () => {
  for (const available of [true, false]) {
    const f = entryFixture(available);
    f.message({ type: "prepare" });
    assert.equal(f.host.animationFrameSource, available ? "worker" : "timer");
    if (available) assert.equal(f.host.requestAnimationFrame(() => {}), 77);
    else assert.equal(f.host.requestAnimationFrame, null);
    assert.equal(f.sent.length, 0);
    f.host.close();
    assert.equal(f.self.closed, true);
  }
});
