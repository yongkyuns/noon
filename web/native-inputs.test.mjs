import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  attachNativeInputs,
  bindNativeControl,
  createExecutionWorkerNativeInputHost,
} from "./native-inputs.js";

class FakeTarget {
  listeners = new Map();

  addEventListener(type, listener, { signal } = {}) {
    signal?.addEventListener("abort", () => this.removeEventListener(type, listener), { once: true });
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  removeEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? [];
    this.listeners.set(
      type,
      listeners.filter((candidate) => candidate !== listener),
    );
  }

  dispatch(type, event) {
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }
}

class FakeCanvas extends FakeTarget {
  constructor({ lineHeight = "20px", fontSize = "16px" } = {}) {
    super();
    this.style = { lineHeight, fontSize };
    this.ownerDocument = {
      defaultView: Object.assign(new FakeTarget(), {
        getComputedStyle: () => this.style, devicePixelRatio: 1,
      }),
    };
  }

  getBoundingClientRect() {
    return { left: 0, top: 0, width: 640, height: 360 };
  }
}

function wheelEvent(deltaX, deltaY, deltaMode) {
  return {
    deltaX,
    deltaY,
    deltaMode,
    defaultPrevented: false,
    preventDefault() {
      this.defaultPrevented = true;
    },
  };
}

function recordingHost(overrides = {}) {
  const wheel = [];
  return {
    wheel,
    nativePointerInput() {},
    nativeKey() {},
    nativeWheel(x, y) {
      wheel.push([x, y]);
    },
    nativeControl() {},
    nativeControlCommit() {},
    ...overrides,
  };
}

test("native wheel input normalizes browser delta modes to CSS pixels", () => {
  const canvas = new FakeCanvas();
  const keyboardTarget = new FakeTarget();
  const player = recordingHost();
  const detach = attachNativeInputs(player, canvas, {
    keyboardTarget,
    preventWheelDefault: true,
  });

  const pixels = wheelEvent(2, -3, 0);
  canvas.dispatch("wheel", pixels);
  assert.equal(pixels.defaultPrevented, true);

  const lines = wheelEvent(2, -3, 1);
  canvas.dispatch("wheel", lines);

  const pages = wheelEvent(0.5, -0.25, 2);
  canvas.dispatch("wheel", pages);

  assert.deepEqual(player.wheel, [
    [2, -3],
    [40, -60],
    [320, -90],
  ]);

  detach();
  canvas.dispatch("wheel", wheelEvent(1, 1, 0));
  assert.equal(player.wheel.length, 3, "detached collectors must stop forwarding wheel input");
});

test("line-mode wheel input falls back to computed font size for normal line-height", () => {
  const canvas = new FakeCanvas({ lineHeight: "normal", fontSize: "18px" });
  const player = recordingHost();
  const detach = attachNativeInputs(player, canvas, {
    keyboardTarget: new FakeTarget(),
  });

  canvas.dispatch("wheel", wheelEvent(1.5, -2, 1));
  assert.deepEqual(player.wheel, [[27, -36]]);
  detach();
});

test("unknown browser wheel delta modes fail instead of silently changing units", () => {
  const canvas = new FakeCanvas();
  const player = recordingHost();
  const detach = attachNativeInputs(player, canvas, {
    keyboardTarget: new FakeTarget(),
  });

  assert.throws(
    () => canvas.dispatch("wheel", wheelEvent(1, 1, 9)),
    /unsupported WheelEvent\.deltaMode 9/,
  );
  assert.deepEqual(player.wheel, []);
  detach();
});

test("canonical DOM input delivers one occurrence instead of split state and edge calls", async () => {
  const calls = [];
  const client = {
    submitBrowserPointerInput(input) { calls.push(["pointer", input]); return Promise.resolve(); },
    setNativeStateInput(source, value) {
      calls.push(["state", source, value]);
      return Promise.resolve();
    },
    emitNativeEvent(source) {
      calls.push(["event", source]);
      return Promise.resolve();
    },
  };
  const host = createExecutionWorkerNativeInputHost(client);
  const canvas = new FakeCanvas();
  const errors = [];
  const detach = attachNativeInputs(host, canvas, {
    keyboardTarget: new FakeTarget(),
    onError: (error) => errors.push(String(error)),
  });

  canvas.dispatch("pointerdown", { clientX: 320, clientY: 90, button: 0, buttons: 1, isPrimary: true, pointerId: 4, pointerType: "mouse" });
  await Promise.resolve();

  assert.deepEqual(calls, [["pointer", {
    kind: "press", source_id: 1, pointer_id: 4, view_revision: 0,
    surface_x: 320, surface_y: 90, viewport_width: 640, viewport_height: 360,
    button: 0, shift: false, control: false, alt: false, meta: false,
  }]]);
  assert.deepEqual(errors, []);
  detach();
});

test("worker native ingress rejects synchronously at its in-flight bound without queuing", async () => {
  const pending = [];
  const client = {
    submitBrowserPointerInput() { return new Promise(resolve => pending.push(resolve)); },
    setNativeStateInput() {
      return new Promise((resolve) => pending.push(resolve));
    },
    emitNativeEvent() {
      return new Promise((resolve) => pending.push(resolve));
    },
  };
  const host = createExecutionWorkerNativeInputHost(client, {
    maxInFlight: 2,
  });

  const accepted = host.nativeKey("Space", true);
  await assert.rejects(
    host.nativeControl("opacity", 0.5),
    /native input admission is full/,
  );
  assert.equal(pending.length, 2, "overflow must not enqueue another worker request");
  pending.splice(0).forEach((resolve) => resolve());
  await accepted;
  const retried = host.nativeControl("opacity", 0.5);
  assert.equal(pending.length, 1);
  pending.splice(0).forEach((resolve) => resolve());
  await retried;
});

test("listener and control async failures surface through the configured error path", async () => {
  const failure = new Error("semantic native input rejected");
  const host = recordingHost({
    nativeKey: () => Promise.reject(failure),
    nativeControl: () => Promise.reject(failure),
  });
  const keyboardTarget = new FakeTarget();
  const canvas = new FakeCanvas();
  const errors = [];
  const onError = (error) => errors.push(error);
  const detachInputs = attachNativeInputs(host, canvas, { keyboardTarget, onError });
  const control = new FakeTarget();
  control.value = "0.5";
  const detachControl = bindNativeControl(host, control, "opacity", { onError });

  keyboardTarget.dispatch("keydown", { code: "Space" });
  await Promise.resolve();
  await Promise.resolve();
  assert.deepEqual(errors, [failure, failure]);

  detachInputs();
  detachControl();
});

test("DOM collector and direct canvas expose canonical native ingress", async () => {
  const [collector, directCanvas] = await Promise.all([
    readFile(new URL("./native-inputs.js", import.meta.url), "utf8"),
    readFile(new URL("../crates/noon-web/src/execution_canvas.rs", import.meta.url), "utf8"),
  ]);
  assert.ok(collector.includes("createExecutionWorkerNativeInputHost"));
  assert.ok(collector.includes("attachBrowserPointerInput"));
  assert.ok(!collector.includes("pointerToScene"));
  for (const method of [
    "nativePointerInput",
    "nativeKey",
    "nativeWheel",
    "nativeControl",
    "nativeControlCommit",
  ]) {
    assert.ok(directCanvas.includes(method), `direct typed canvas is missing ${method}`);
  }
  assert.ok(!directCanvas.includes("normalized_pointer_world_position"));
  assert.ok(!directCanvas.includes("nativePointerButton"));
  assert.ok(directCanvas.includes("submit_browser_pointer_input"));
  assert.ok(directCanvas.includes("NativeEventOccurrence::new"));
});

function pointer(type, x, buttons = 0) {
  return { pointerId: 7, pointerType: "mouse", isPrimary: true, clientX: x,
    clientY: 100, buttons, button: type === "pointermove" ? -1 : 0,
    timeStamp: 10 };
}

test("direct attachment forwards every coalesced excursion and one occurrence per edge", () => {
  const canvas = new FakeCanvas(), calls = [], accepted = [];
  const host = recordingHost({ nativePointerInput: (...args) => { calls.push(args); return false; } });
  const detach = attachNativeInputs(host, canvas, { keyboardTarget: new FakeTarget(),
    onInput: value => accepted.push(value) });
  canvas.dispatch("pointerdown", pointer("pointerdown", 100, 1));
  const move = pointer("pointermove", 100, 1);
  move.getCoalescedEvents = () => [pointer("pointermove", 180, 1), pointer("pointermove", 100, 1)];
  canvas.dispatch("pointermove", move);
  canvas.dispatch("pointerup", pointer("pointerup", 100, 0));
  assert.deepEqual(calls.map(args => [args[0], args[4]]), [["press", 100], ["move", 180], ["move", 100], ["release", 100]]);
  assert.equal(accepted.length, 4);
  detach();
});

test("direct reattachment retires callbacks and monotonically replaces source lifetime", () => {
  const canvas = new FakeCanvas(), calls = [];
  const host = recordingHost({ nativePointerInput: (...args) => calls.push(args) });
  let detach = attachNativeInputs(host, canvas, { keyboardTarget: new FakeTarget() });
  const stale = canvas.listeners.get("pointerdown")[0];
  canvas.dispatch("pointerdown", pointer("pointerdown", 100, 1));
  const first = calls[0][1];
  detach();
  assert.equal(calls.at(-1)[0], "cancel");
  detach = attachNativeInputs(host, canvas, { keyboardTarget: new FakeTarget() });
  const before = calls.length;
  stale(pointer("pointerdown", 100, 1));
  assert.equal(calls.length, before);
  canvas.dispatch("pointerdown", pointer("pointerdown", 100, 1));
  assert.ok(calls.at(-1)[1] > first);
  detach();
});

test("rejected direct input neither notifies the wake driver nor admits a later release", () => {
  const canvas = new FakeCanvas(), errors = [], calls = [], wakes = [];
  const host = recordingHost({ nativePointerInput: (...args) => {
    calls.push(args); throw new Error("admission rejected");
  } });
  const detach = attachNativeInputs(host, canvas, { keyboardTarget: new FakeTarget(),
    onError: error => errors.push(error), onInput: value => wakes.push(value) });
  canvas.dispatch("pointerdown", pointer("pointerdown", 100, 1));
  canvas.dispatch("pointerup", pointer("pointerup", 100, 0));
  assert.deepEqual(calls.map(args => args[0]), ["press", "cancel"],
    "one cleanup attempt is allowed, but the rejected press and later release are never replayed");
  assert.equal(errors.length, 1); assert.equal(wakes.length, 0);
  assert.match(String(errors[0]), /admission rejected/);
  detach();
  assert.equal(calls.length, 2, "cleanup is not retried after retirement");
});

test("detached asynchronous input cannot wake a replacement attachment", async () => {
  const canvas = new FakeCanvas(), keyboardTarget = new FakeTarget(), wakes = [];
  let complete;
  const host = recordingHost({ nativeKey: () => new Promise(resolve => { complete = resolve; }) });
  const detach = attachNativeInputs(host, canvas, { keyboardTarget, onInput: value => wakes.push(value) });
  keyboardTarget.dispatch("keydown", { code: "Space" });
  detach();
  const replacement = attachNativeInputs(host, canvas, { keyboardTarget, onInput: value => wakes.push(value) });
  complete(true);
  await Promise.resolve();
  assert.deepEqual(wakes, []);
  replacement();
});

test("asynchronous presentation notification failure retires input and reports the error", async () => {
  const canvas = new FakeCanvas(), keyboardTarget = new FakeTarget(), errors = [];
  const failure = new Error("presentation failed");
  let calls = 0;
  const host = recordingHost({ nativeKey: () => { calls++; return Promise.resolve(true); } });
  const detach = attachNativeInputs(host, canvas, { keyboardTarget,
    onInput: () => { throw failure; }, onError: error => errors.push(error) });
  keyboardTarget.dispatch("keydown", { code: "Space" });
  await Promise.resolve();
  keyboardTarget.dispatch("keyup", { code: "Space" });
  assert.equal(calls, 1);
  assert.deepEqual(errors, [failure]);
  assert.ok([...canvas.listeners.values()].every(listeners => listeners.length === 0));
  assert.ok([...keyboardTarget.listeners.values()].every(listeners => listeners.length === 0));
  detach();
});

test("authoring facade with its own pointer collector accepts nonpointer controls only", async () => {
  const calls = [];
  const client = {
    setNativeStateInput(source, value) { calls.push(["state", source, value]); return Promise.resolve(); },
    emitNativeEvent(source) { calls.push(["event", source]); return Promise.resolve(); },
  };
  const host = createExecutionWorkerNativeInputHost(client);
  assert.equal(host.nativePointerInput, undefined, "do not fabricate a second pointer ingress");
  const canvas = new FakeCanvas(), keyboardTarget = new FakeTarget();
  assert.throws(() => attachNativeInputs(host, canvas, { keyboardTarget }), /nativePointerInput/);
  const detach = attachNativeInputs(host, canvas, { pointer: false, keyboardTarget });
  keyboardTarget.dispatch("keydown", { code: "Space" });
  canvas.dispatch("wheel", wheelEvent(1, 2, 0));
  const control = new FakeTarget(); control.value = "0.5";
  const detachControl = bindNativeControl(host, control, "opacity");
  control.dispatch("input", {});
  await Promise.resolve();
  assert.equal(calls.length, 6);
  assert.ok(calls.some(([kind, source]) => kind === "state" && source.kind === "key"));
  assert.ok(calls.some(([kind, source]) => kind === "state" && source.kind === "control"));
  assert.ok(!(canvas.listeners.get("pointerdown")?.length));
  detach(); detachControl();
});

test("control-only binding does not require unrelated pointer keyboard or wheel APIs", () => {
  const calls = [], control = new FakeTarget(); control.value = "0.5";
  const detach = bindNativeControl({
    nativeControl: (...args) => calls.push(["value", ...args]),
    nativeControlCommit: (...args) => calls.push(["commit", ...args]),
  }, control, "opacity");
  control.dispatch("change", {});
  assert.deepEqual(calls, [["value", "opacity", 0.5], ["value", "opacity", 0.5], ["commit", "opacity"]]);
  detach();
});

test("fatal presentation notification after an admitted press cancels the contact before detaching", () => {
  const canvas = new FakeCanvas(), keyboardTarget = new FakeTarget();
  const calls = [], errors = [];
  const failure = new Error("presentation notification failed after input commit");
  let held = false;
  const host = recordingHost({ nativePointerInput: (...args) => {
    calls.push(args[0]);
    if (args[0] === "press") held = true;
    if (args[0] === "cancel") held = false;
    return true;
  } });
  const detach = attachNativeInputs(host, canvas, { keyboardTarget,
    onInput: () => { throw failure; }, onError: error => errors.push(error) });
  canvas.dispatch("pointerdown", pointer("pointerdown", 100, 1));
  assert.deepEqual(calls, ["press", "cancel"]);
  assert.equal(held, false, "retiring listeners must not strand an admitted button");
  assert.deepEqual(errors, [failure]);
  canvas.dispatch("pointerup", pointer("pointerup", 100, 0));
  detach();
  assert.deepEqual(calls, ["press", "cancel"], "neither release nor cleanup is replayed");
});

test("malformed later motion cancels a previously admitted contact before failing closed", () => {
  const canvas = new FakeCanvas(), keyboardTarget = new FakeTarget();
  const calls = [], errors = [];
  const host = recordingHost({ nativePointerInput: (...args) => calls.push(args[0]) });
  const detach = attachNativeInputs(host, canvas, { keyboardTarget,
    onError: error => errors.push(error) });
  canvas.dispatch("pointerdown", pointer("pointerdown", 100, 1));
  canvas.dispatch("pointermove", { ...pointer("pointermove", 100, 1), clientX: NaN });
  assert.deepEqual(calls, ["press", "cancel"]);
  assert.equal(errors.length, 1);
  assert.match(String(errors[0]), /finite/);
  assert.ok([...canvas.listeners.values()].every(listeners => listeners.length === 0));
  detach();
});

test("asynchronous notification failure cancels once and cannot notify a replacement attachment", async () => {
  const canvas = new FakeCanvas(), keyboardTarget = new FakeTarget();
  const calls = [], errors = [], replacementWakes = [];
  const failure = new Error("asynchronous presentation notification failed");
  const host = recordingHost({ nativePointerInput: (...args) => {
    calls.push(args);
    return Promise.resolve(true);
  } });
  const detach = attachNativeInputs(host, canvas, { keyboardTarget,
    onInput: () => { throw failure; }, onError: error => errors.push(error) });
  canvas.dispatch("pointerdown", pointer("pointerdown", 100, 1));
  await Promise.resolve(); await Promise.resolve();
  assert.deepEqual(calls.map(args => args[0]), ["press", "cancel"]);
  assert.deepEqual(errors, [failure]);
  detach();
  const replacement = attachNativeInputs(host, canvas, { keyboardTarget,
    onInput: value => replacementWakes.push(value) });
  await Promise.resolve();
  assert.deepEqual(replacementWakes, []);
  canvas.dispatch("pointerdown", pointer("pointerdown", 100, 1));
  await Promise.resolve();
  assert.ok(calls.at(-1)[1] > calls[0][1]);
  assert.deepEqual(replacementWakes, [true]);
  replacement();
});

test("collector delivery can synchronously retire its contact without a secondary exception", async () => {
  const { attachBrowserPointerInput } = await import("./browser-pointer-input.js");
  const canvas = new FakeCanvas(), controller = new AbortController();
  const calls = [], errors = [];
  let source = 0;
  const collector = attachBrowserPointerInput(canvas, {
    signal: controller.signal, isCurrent: () => true,
    allocateSource: () => ++source, viewRevision: () => 0, advanceView: () => {},
    windowTarget: canvas.ownerDocument.defaultView, maxSamples: 64,
    onError: error => errors.push(error),
    send: sample => {
      calls.push(sample.kind);
      if (sample.kind === "press") {
        collector.invalidateView();
        controller.abort();
      }
    },
  });
  canvas.dispatch("pointerdown", pointer("pointerdown", 100, 1));
  assert.deepEqual(calls, ["press", "cancel"]);
  assert.deepEqual(errors, [], "returning delivery must not dereference a retired contact");
});
