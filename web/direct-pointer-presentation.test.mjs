import assert from "node:assert/strict";
import test from "node:test";
import { attachNativeInputs } from "./native-inputs.js";

class Target {
  listeners = new Map();
  addEventListener(type, fn, { signal } = {}) {
    const values = this.listeners.get(type) ?? []; values.push(fn); this.listeners.set(type, values);
    signal?.addEventListener("abort", () => this.removeEventListener(type, fn), { once: true });
  }
  removeEventListener(type, fn) { this.listeners.set(type, (this.listeners.get(type) ?? []).filter(x => x !== fn)); }
  emit(type, value = {}) { for (const fn of [...this.listeners.get(type) ?? []]) fn(value); }
}
function fixture({ reject = () => false } = {}) {
  const window = Object.assign(new Target(), { devicePixelRatio: 2 });
  const canvas = Object.assign(new Target(), { ownerDocument: { defaultView: window },
    rect: { left: 10, top: 20, width: 800, height: 400 },
    getBoundingClientRect() { return { ...this.rect }; } });
  const calls = [], errors = [], notifications = [];
  const host = {
    setPointerView(...args) { calls.push(["view", ...args]); return true; },
    nativePointerInput(...args) { calls.push(args); return reject(args) ? undefined : false; },
    nativeKey() {}, nativeWheel() {},
  };
  const attach = () => attachNativeInputs(host, canvas, { keyboardTarget: window,
    onInput: value => notifications.push(value), onError: error => errors.push(error) });
  let detach = attach();
  const event = (type, buttons = 0) => canvas.emit(type, { pointerId: 7, pointerType: "mouse", isPrimary: true,
    clientX: canvas.rect.left + 400, clientY: canvas.rect.top + 200,
    buttons, button: type === "pointermove" ? -1 : 0 });
  return { canvas, window, calls, errors, notifications, event,
    detach: () => detach(), reattach: () => { detach(); detach = attach(); } };
}

test("direct view is registered before any occurrence without scene/publication IDs", () => {
  const f = fixture();
  assert.deepEqual(f.calls, [["view", 1, 800, 400]]);
  f.event("pointerdown", 1); f.event("pointerup", 0);
  assert.deepEqual(f.calls.map(x => x[0]), ["view", "press", "release"]);
  assert.deepEqual(f.calls[1].slice(1, 4), [1, 7, 1]);
  f.detach(); assert.deepEqual(f.errors, []);
});

test("admitted clean false result does not retire the contact", () => {
  const f = fixture(); f.event("pointerdown", 1); f.event("pointerup");
  assert.deepEqual(f.calls.map(x => x[0]), ["view", "press", "release"]);
  assert.equal(f.calls[1][1], f.calls[2][1]); f.detach();
});

test("recoverable press rejection does not deliver held motion or release and allows a new source", async () => {
  let reject = true; const f = fixture({ reject: () => reject });
  await Promise.resolve();
  f.event("pointerdown", 1); f.event("pointermove", 1); f.event("pointerup");
  assert.deepEqual(f.calls.map(x => x[0]), ["view", "press"]);
  await Promise.resolve();
  assert.equal(f.notifications.at(-1), undefined, "still notify redraw on rejection");
  reject = false; f.event("pointerdown", 1); f.event("pointerup");
  assert.deepEqual(f.calls.map(x => x[0]), ["view", "press", "press", "release"]);
  assert.ok(f.calls[2][1] > f.calls[1][1]); assert.deepEqual(f.errors, []); f.detach();
});

test("rejected held motion retires the contact without a second cancellation", () => {
  const f = fixture({ reject: args => args[0] === "move" });
  f.event("pointerdown", 1); f.event("pointermove", 1); f.event("pointerup");
  assert.deepEqual(f.calls.map(x => x[0]), ["view", "press", "move"]);
  assert.deepEqual(f.errors, []); f.detach();
});

test("scroll mapping change cancels before registering a new same-sized view", () => {
  const f = fixture(); f.event("pointerdown", 1);
  f.canvas.rect.left += 15; f.window.emit("scroll"); f.event("pointerup");
  assert.deepEqual(f.calls.map(x => x[0]), ["view", "press", "cancel", "view"]);
  assert.deepEqual(f.calls.at(-1), ["view", 2, 800, 400]);
  f.event("pointerdown", 1); assert.equal(f.calls.at(-1)[3], 2); f.detach();
});

test("unchanged resize observations do not retire receipts or allocate sources", () => {
  const f = fixture(); f.event("pointerdown", 1);
  for (let i = 0; i < 128; ++i) { f.window.emit("resize"); f.window.emit("scroll"); }
  f.event("pointerup"); assert.deepEqual(f.calls.map(x => x[0]), ["view", "press", "release"]);
  assert.equal(f.calls[1][1], f.calls[2][1]); f.detach();
});

test("DPR change invalidates the same CSS dimensions before new input", () => {
  const f = fixture(); f.window.devicePixelRatio = 3; f.window.emit("resize");
  assert.deepEqual(f.calls, [["view", 1, 800, 400], ["view", 2, 800, 400]]); f.detach();
});

test("reattachment cannot reuse the old view or pointer source", () => {
  const f = fixture(); f.event("pointerdown", 1); const old = f.calls.at(-1);
  f.reattach(); f.event("pointerup"); f.event("pointerdown", 1); const next = f.calls.at(-1);
  assert.ok(next[1] > old[1]); assert.ok(next[3] > old[3]);
  assert.deepEqual(f.calls.map(x => x[0]), ["view", "press", "cancel", "view", "view", "press"]);
  f.detach(); const count = f.calls.length; f.window.emit("scroll"); f.window.emit("resize");
  assert.equal(f.calls.length, count); assert.deepEqual(f.errors, []);
});

test("hidden view clears the platform mapping and suppresses input", () => {
  const f = fixture(); f.event("pointerdown", 1);
  f.canvas.rect.width = 0; f.window.emit("resize"); f.event("pointerup");
  assert.ok(f.calls.some(x => x[0] === "view" && x[2] === 0 && x[3] === 0));
  assert.ok(!f.calls.some(x => x[0] === "release")); f.detach();
});

test("same-callback samples cannot refresh the receipt through synchronous notifications", async () => {
  const f = fixture(); await Promise.resolve();
  const initial = f.notifications.length;
  f.event("pointerdown", 1); f.event("pointermove", 1); f.event("pointerup");
  assert.equal(f.notifications.length, initial);
  await Promise.resolve(); assert.equal(f.notifications.length, initial + 1);
  f.detach();
});
