import assert from "node:assert/strict";
import test from "node:test";
import { observePointerLatency } from "./pointer-latency-observer.mjs";

globalThis.document = undefined;

function fixture(t, capacity) {
  const listeners = new Map();
  const document = {
    addEventListener(type, listener) { listeners.set(type, listener); },
    removeEventListener(type) { listeners.delete(type); },
  };
  class Port extends EventTarget {
    messages = [];
    postMessage(message) { this.messages.push(message); if (this.failure) throw this.failure; }
    reply(message) { this.dispatchEvent(new MessageEvent("message", { data: { channel: "noon.engine", ...message } })); }
  }
  const canvas = {};
  let resolve, reject;
  const result = new Promise((yes, no) => { resolve = yes; reject = no; });
  class Client {
    canvas = canvas;
    submitBrowserPointerInput() { return result; }
  }
  t.mock.property(globalThis, "document", document);
  t.mock.property(globalThis, "MessagePort", Port);
  let now = 100;
  t.mock.method(performance, "now", () => now);
  const originalSubmit = Client.prototype.submitBrowserPointerInput;
  const originalPost = Port.prototype.postMessage;
  const observer = observePointerLatency(Client, { clockId: "navigation-1", capacity });
  t.after(() => observer.stop());
  const port = new Port(), client = new Client();
  const input = { kind: "release", source_id: 4, pointer_id: 9, view_revision: 2, surface_x: 50, surface_y: 40 };
  return { observer, port, client, input, result, resolve, reject, originalSubmit, originalPost,
    clock: value => { now = value; },
    dom() {
      const event = { type: "pointerup", isPrimary: true, button: 0, pointerId: 9,
        currentTarget: document, eventPhase: 1, target: { closest: () => canvas } };
      listeners.get("pointerup")(event);
      event.currentTarget = canvas; event.eventPhase = 2;
      return event;
    },
    post(id = 1) { port.postMessage({ channel: "noon.engine", type: "browser_pointer_input", requestId: id,
      input: structuredClone(input) }); },
    snapshot: () => observer.snapshot({ runStatus: "completed" }),
  };
}

test("collector preserves the product promise and joins DOM, exact post, reply and deferred control", async t => {
  const f = fixture(t);
  f.observer.action("restart-1");
  f.dom();
  await Promise.resolve(); // Browsers can checkpoint between capture and target.
  assert.equal(f.client.submitBrowserPointerInput(f.input), f.result);
  f.clock(102); f.post();
  f.clock(105); f.port.reply({ type: "browser_pointer_input", requestId: 1, pointerInputAccepted: true,
    sceneControl: { sequence: "9007199254740993", action: "toggle_playback" } });
  f.clock(106); f.port.postMessage({ channel: "noon.engine", type: "scene_control", requestId: 2,
    sequence: "9007199254740993" });
  f.clock(109); f.port.reply({ type: "scene_control", requestId: 2, sceneControlApplied: true });
  const receipt = { session: 1, sequence: 7, presentation: 3, view_revision: 2 };
  f.clock(110); f.port.reply({ type: "pointer_presented", receipt });
  f.port.reply({ type: "scene_control", requestId: 2, sceneControlApplied: false });
  f.port.reply({ type: "scene_control", requestId: 999, sceneControlApplied: false });
  f.resolve({ ok: true }); await f.result;
  const trace = f.snapshot(), [row] = trace.attempts;
  assert.deepEqual([row.domMs, row.postedMs, row.replyMs, row.controlPostedMs, row.controlReplyMs],
    [100, 102, 105, 106, 109]);
  assert.deepEqual([row.requestId, row.controlRequestId, row.outcome, row.applied], [1, 2, "accepted", true]);
  assert.deepEqual(trace.presentations, [{ endpoint: row.endpoint, clockId: "navigation-1", atMs: 110, receipt }]);
  assert.equal(row.presentedMs, undefined, "a nearby presentation cannot be assigned to a control");
  assert.equal(trace.presentationJoin, "unknown");
  assert.deepEqual(trace.observerErrors, []);
  assert.deepEqual(f.port.messages[0].input, f.input);
  assert.equal(f.port.messages.length, 2, "observation cannot send extra messages");
});

test("collector retains rejection, missing submissions, and observer capacity loss", async t => {
  const f = fixture(t, 2);
  f.dom(); f.client.submitBrowserPointerInput(f.input); f.post();
  f.port.reply({ type: "browser_pointer_input", requestId: 1, pointerInputAccepted: false });
  await Promise.resolve();
  f.dom(); // A physically observed release suppressed by the normal collector.
  await Promise.resolve();
  f.dom(); // Capacity loss is explicit, never a faster zero-latency sample.
  const trace = f.snapshot();
  assert.deepEqual(trace.attempts.map(row => row.outcome), ["rejected", "pending"]);
  assert.equal(trace.attempts[1].postedMs, undefined);
  assert.equal(trace.dropped, 1);
  f.resolve();
});

test("ambiguous exact inputs are retained without guessing their request association", async t => {
  const f = fixture(t);
  f.dom(); f.client.submitBrowserPointerInput(f.input);
  f.dom(); f.client.submitBrowserPointerInput(f.input);
  f.post();
  const trace = f.snapshot();
  assert.equal(trace.attempts.length, 2);
  assert.ok(trace.attempts.every(row => row.postedMs === undefined));
  assert.match(trace.observerErrors[0], /one exact collected occurrence/);
  f.resolve();
});

test("a later asynchronous submission cannot inherit an ended DOM dispatch", t => {
  const f = fixture(t);
  const event = f.dom();
  event.currentTarget = null; event.eventPhase = 0;
  f.client.submitBrowserPointerInput(f.input); f.post();
  const trace = f.snapshot();
  assert.equal(trace.attempts[0].postedMs, undefined);
  assert.match(trace.observerErrors[0], /one exact collected occurrence/);
  f.resolve();
});

test("product cancellation is preserved and stopping removes only the installed observers", async t => {
  const f = fixture(t);
  f.dom();
  const product = f.client.submitBrowserPointerInput(f.input);
  f.post();
  const failure = new Error("retired endpoint"); failure.name = "AbortError";
  f.reject(failure);
  await assert.rejects(product, error => error === failure);
  await Promise.resolve();
  assert.equal(f.snapshot().attempts[0].outcome, "cancelled");
  f.observer.stop();
  assert.equal(f.client.submitBrowserPointerInput, f.originalSubmit);
  assert.equal(f.port.postMessage, f.originalPost);
});

test("a failed native post remains an attempt without a fabricated successful send", t => {
  const f = fixture(t);
  f.dom(); f.client.submitBrowserPointerInput(f.input);
  f.port.failure = new Error("structured clone failed");
  assert.throws(() => f.post(), error => error === f.port.failure);
  const [row] = f.snapshot().attempts;
  assert.equal(row.outcome, "error");
  assert.equal(row.postAttemptMs, 100);
  assert.equal(row.issuedRequestId, 1);
  assert.equal(row.postedMs, undefined);
  assert.equal(row.requestId, undefined);
  assert.equal(f.port.messages.length, 1);
  f.resolve();
});
