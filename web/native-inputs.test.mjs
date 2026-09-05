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

  addEventListener(type, listener) {
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
      defaultView: {
        getComputedStyle: () => this.style,
      },
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
    nativePointerPosition() {},
    nativePointerButton() {},
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

test("canonical DOM input preserves sampled-state then ordered-event delivery", async () => {
  const calls = [];
  const client = {
    setNativeStateInput(source, value) {
      calls.push(["state", source, value]);
      return Promise.resolve();
    },
    emitNativeEvent(source) {
      calls.push(["event", source]);
      return Promise.resolve();
    },
  };
  const host = createExecutionWorkerNativeInputHost(client, {
    pointerToScene: (x, y) => ({ x: x * 10, y: -y * 10 }),
  });
  const canvas = new FakeCanvas();
  const errors = [];
  const detach = attachNativeInputs(host, canvas, {
    keyboardTarget: new FakeTarget(),
    onError: (error) => errors.push(String(error)),
  });

  canvas.dispatch("pointerdown", { clientX: 320, clientY: 90, button: 0 });
  await Promise.resolve();

  assert.deepEqual(calls, [
    [
      "state",
      { kind: "pointer_position" },
      { kind: "vec2", x: 5, y: -2.5 },
    ],
    [
      "state",
      { kind: "pointer_button", button: 0 },
      { kind: "bool", value: true },
    ],
    ["event", { kind: "pointer_down", button: 0 }],
  ]);
  assert.deepEqual(errors, []);
  detach();
});

test("worker native ingress rejects synchronously at its in-flight bound without queuing", async () => {
  const pending = [];
  const client = {
    setNativeStateInput() {
      return new Promise((resolve) => pending.push(resolve));
    },
    emitNativeEvent() {
      return new Promise((resolve) => pending.push(resolve));
    },
  };
  const host = createExecutionWorkerNativeInputHost(client, {
    pointerToScene: (x, y) => ({ x, y }),
    maxInFlight: 2,
  });

  const accepted = host.nativePointerButton(0, true);
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
  assert.ok(collector.includes("MAX_PENDING_SEMANTIC_CONTROLS"));
  for (const method of [
    "nativePointerPosition",
    "nativePointerButton",
    "nativeKey",
    "nativeWheel",
    "nativeControl",
    "nativeControlCommit",
  ]) {
    assert.ok(directCanvas.includes(method), `direct typed canvas is missing ${method}`);
  }
  assert.ok(directCanvas.includes("normalized_pointer_world_position"));
  assert.ok(directCanvas.includes("NativeEventOccurrence::new"));
});
