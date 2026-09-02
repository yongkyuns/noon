import assert from "node:assert/strict";
import test from "node:test";

import { attachNativeInputs } from "./native-inputs.js";

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

function recordingPlayer() {
  const wheel = [];
  return {
    wheel,
    dispatchPointerPosition() {},
    dispatchPointerButton() {},
    dispatchKey() {},
    dispatchWheel(x, y) {
      wheel.push([x, y]);
    },
  };
}

test("native wheel input normalizes browser delta modes to CSS pixels", () => {
  const canvas = new FakeCanvas();
  const keyboardTarget = new FakeTarget();
  const player = recordingPlayer();
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
  const player = recordingPlayer();
  const detach = attachNativeInputs(player, canvas, {
    keyboardTarget: new FakeTarget(),
  });

  canvas.dispatch("wheel", wheelEvent(1.5, -2, 1));
  assert.deepEqual(player.wheel, [[27, -36]]);
  detach();
});

test("unknown browser wheel delta modes fail instead of silently changing units", () => {
  const canvas = new FakeCanvas();
  const player = recordingPlayer();
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
