import assert from "node:assert/strict";
import test from "node:test";
import { attachSpatialInspectionInput } from "./spatial-inspection-input.js";

function fixture(t, options) {
  const previousWindow = globalThis.window;
  globalThis.window = new EventTarget();
  const controller = new AbortController();
  const captured = new Set();
  const canvas = Object.assign(new EventTarget(), {
    getBoundingClientRect: () => ({ left: 0, top: 0, height: 100 }),
    setPointerCapture: id => captured.add(id),
    hasPointerCapture: id => captured.has(id),
    releasePointerCapture: id => captured.delete(id),
  });
  t.after(() => {
    controller.abort();
    globalThis.window = previousWindow;
  });
  attachSpatialInspectionInput(canvas, {
    signal: controller.signal, enabled: () => true, viewRevision: () => 3,
    presentation: () => null, onError: error => assert.fail(error), ...options,
  });
  const pointer = (type, clientX, extra = {}) => canvas.dispatchEvent(Object.assign(new Event(type, { cancelable: true }), {
    pointerId: 1, pointerType: "mouse", button: 0, clientX, clientY: 25, ...extra,
  }));
  return { canvas, captured, pointer };
}

test("the collector preserves ordered gesture edges and captures the release receipt immediately", async t => {
  let enabled = false;
  let spatial = false;
  let finishPress;
  const firstDelivery = new Promise(resolve => { finishPress = resolve; });
  const sent = [];
  let ordinaryPresses = 0;
  let ordinaryCaptureLosses = 0;
  let receipt = { sequence: 3 };
  const { canvas, captured, pointer } = fixture(t, {
    enabled: () => enabled, spatial: () => spatial, presentation: () => receipt,
    async send(packet) {
      sent.push(packet);
      if (sent.length === 1) await firstDelivery;
    },
  });
  canvas.addEventListener("pointerdown", () => { ordinaryPresses++; });
  canvas.addEventListener("lostpointercapture", () => { ordinaryCaptureLosses++; });

  assert.equal(pointer("pointerdown", 20), true);
  enabled = true;
  assert.equal(pointer("pointerdown", 20), true, "planar primary gestures reach the scene");
  assert.equal(pointer("lostpointercapture", 20), true);
  assert.equal(ordinaryCaptureLosses, 1);
  spatial = true;
  assert.equal(pointer("pointerdown", 20), false);
  assert.equal(ordinaryPresses, 2);
  assert.ok(captured.has(1));
  pointer("pointermove", 25);
  pointer("pointermove", 35);
  pointer("pointerup", 45);
  assert.equal(sent.length, 1, "one worker request can be in flight");
  assert.equal(captured.size, 0);
  receipt = { sequence: 12 };
  finishPress();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(sent.map(packet => packet.samples.map(sample => [sample.phase, sample.x])),
    [[["press", 0.2]], [["move", 0.25], ["move", 0.35]], [["release", 0.45]]]);
  assert.deepEqual(sent.at(-1).presentation, { sequence: 3 });
});

test("overflow cancels an admitted press whose release was already queued", async t => {
  let finishPress;
  const firstDelivery = new Promise(resolve => { finishPress = resolve; });
  const sent = [];
  const { captured, pointer } = fixture(t, {
    async send(packet) {
      sent.push(packet);
      if (sent.length === 1) await firstDelivery;
    },
  });
  pointer("pointerdown", 20);
  pointer("pointerup", 20);
  assert.equal(captured.size, 0);

  for (let i = 0; i < 16; i++) {
    const contact = { pointerId: i + 2 };
    pointer("pointerdown", 20, contact);
    pointer("pointerup", 20, contact);
  }
  finishPress();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(sent[0].samples[0].phase, "press");
  assert.deepEqual(sent[1], { view_revision: 3, cancel_contacts: true });
  assert.equal(captured.size, 0);
});
