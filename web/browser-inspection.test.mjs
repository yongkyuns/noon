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
  emit(type, event) { for (const fn of [...this.listeners.get(type) ?? []]) fn(event); }
}
function fixture({ result = () => true, inspectionZoom = true } = {}) {
  const win = Object.assign(new Target(), { devicePixelRatio: 3, getComputedStyle: () => ({ lineHeight: "20px" }) });
  const canvas = Object.assign(new Target(), { ownerDocument: { defaultView: win },
    rect: { left: 10, top: 20, width: 800, height: 400 }, getBoundingClientRect() { return { ...this.rect }; } });
  const calls = [], errors = [], wakes = [];
  const host = {
    nativeKey() {}, nativeWheel: (...args) => calls.push(["semanticWheel", ...args]),
    nativePointerInput: (...args) => { calls.push(args); return false; },
    setPointerView: (...args) => { calls.push(["view", ...args]); return true; },
    nativeInspectionScroll: (...args) => { calls.push(["scroll", ...args]); return result(); },
  };
  const detach = attachNativeInputs(host, canvas, { keyboardTarget: win, inspectionZoom,
    onError: error => errors.push(error), onInput: value => wakes.push(value) });
  function wheel(overrides = {}) {
    const event = { clientX: 510, clientY: 120, deltaMode: 0, deltaX: 0, deltaY: -100,
      defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...overrides };
    canvas.emit("wheel", event); return event;
  }
  function pointer(type, buttons) { canvas.emit(type, { pointerId: 7, pointerType: "mouse", isPrimary: true,
    clientX: 410, clientY: 220, buttons, button: type === "pointermove" ? -1 : 0 }); }
  return { win, canvas, calls, errors, wakes, host, detach, wheel, pointer };
}

test("inspection is opt-in and does not duplicate semantic wheel input", () => {
  const f = fixture(); assert.equal(f.wheel().defaultPrevented, true);
  assert.deepEqual(f.calls, [["view", 1, 800, 400], ["scroll", 1, 800, 400, 500, 100, -100]]);
  f.detach(); assert.deepEqual(f.errors, []);
  const ordinary = fixture({ inspectionZoom: false });
  assert.equal(ordinary.wheel().defaultPrevented, false);
  assert.deepEqual(ordinary.calls.at(-1), ["semanticWheel", 0, -100]); ordinary.detach();
});

test("pixel line and page modes use CSS units once, not device pixels", () => {
  const f = fixture();
  f.wheel(); f.wheel({ deltaMode: 1, deltaY: -2 }); f.wheel({ deltaMode: 2, deltaY: 0.5 });
  assert.deepEqual(f.calls.filter(x => x[0] === "scroll").map(x => x.at(-1)), [-100, -40, 200]);
  f.detach(); assert.deepEqual(f.errors, []);
});

test("changed inspection retires the DOM press without a duplicate cancel or release", () => {
  const f = fixture(); f.pointer("pointerdown", 1); f.wheel();
  f.pointer("pointermove", 1); f.pointer("pointerup", 0);
  assert.deepEqual(f.calls.map(x => x[0]), ["view", "press", "scroll"]);
  f.pointer("pointerdown", 1); f.pointer("pointerup", 0);
  assert.ok(f.calls[3][1] > f.calls[1][1]); f.detach(); assert.deepEqual(f.errors, []);
});

test("accepted no-op preserves a press; rejected wheel leaves page scrolling available", () => {
  for (const reply of [false, undefined]) {
    const f = fixture({ result: () => reply }); f.pointer("pointerdown", 1);
    assert.equal(f.wheel().defaultPrevented, reply !== undefined); f.pointer("pointerup", 0);
    assert.deepEqual(f.calls.map(x => x[0]), ["view", "press", "scroll", "release"]);
    assert.equal(f.calls[1][1], f.calls[3][1]); f.detach();
  }
});

test("same-task wheel bursts cannot repaint and acquire a newer receipt mid-packet", async () => {
  let first = true;
  const f = fixture({ result: () => { const result = first ? true : undefined; first = false; return result; } });
  await Promise.resolve(); const count = f.wakes.length;
  for (let i = 0; i < 32; ++i) assert.equal(f.wheel().defaultPrevented, i === 0);
  assert.equal(f.wakes.length, count); await Promise.resolve(); assert.equal(f.wakes.length, count + 1);
  assert.equal(f.calls.filter(x => x[0] === "scroll").length, 32); f.detach();
});

test("canvas geometry is registered before wheel dispatch and hidden views do not consume", () => {
  const f = fixture(); f.canvas.rect.left += 20; f.wheel();
  assert.deepEqual(f.calls.slice(-2), [["view", 2, 800, 400], ["scroll", 2, 800, 400, 480, 100, -100]]);
  f.canvas.rect.height = 0; const count = f.calls.filter(x => x[0] === "scroll").length;
  assert.equal(f.wheel().defaultPrevented, false);
  assert.equal(f.calls.filter(x => x[0] === "scroll").length, count); f.detach();
});

test("invalid coordinates modes and overflowing normalized deltas are not delivered or consumed", () => {
  for (const options of [{ clientX: NaN }, { deltaMode: 9 }, { deltaY: Infinity }, { deltaMode: 2, deltaY: Number.MAX_VALUE }]) {
    const f = fixture(); assert.equal(f.wheel(options).defaultPrevented, false);
    assert.ok(f.errors.length === 1); assert.ok(!f.calls.some(x => x[0] === "scroll")); f.detach();
  }
});

test("failed and asynchronous direct delivery cannot consume input or retain listeners", () => {
  for (const result of [() => { throw new Error("admission failed"); }, () => Promise.resolve(true)]) {
    const f = fixture({ result }); assert.equal(f.wheel().defaultPrevented, false);
    assert.equal(f.errors.length, 1); const n = f.calls.length; f.wheel(); assert.equal(f.calls.length, n); f.detach();
  }
});

test("detach removes wheel listeners and new attachment cannot reuse view lifetime", () => {
  const f = fixture(); f.wheel(); f.detach(); const n = f.calls.length;
  assert.equal(f.wheel().defaultPrevented, false); assert.equal(f.calls.length, n);
  const detach = attachNativeInputs(f.host, f.canvas, { keyboardTarget: f.win, inspectionZoom: true });
  assert.ok(f.calls.at(-1)[1] > 1); detach();
});

test("inspection refuses unsupported worker/positionless attachment instead of silently falling back", () => {
  const f = fixture(); f.detach();
  assert.throws(() => attachNativeInputs(f.host, f.canvas, { keyboardTarget: f.win, pointer: false, inspectionZoom: true }), /inspection zoom requires/);
  delete f.host.setPointerView;
  assert.throws(() => attachNativeInputs(f.host, f.canvas, { keyboardTarget: f.win, inspectionZoom: true }), /inspection zoom requires/);
});
