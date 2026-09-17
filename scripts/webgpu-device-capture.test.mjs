import assert from "node:assert/strict";
import test from "node:test";

import { installCaptureState } from "./webgpu-device-capture.mjs";

test("captures the device configured for #scene rather than a probe device", async () => {
  const previous = {
    navigator: globalThis.navigator,
    HTMLCanvasElement: globalThis.HTMLCanvasElement,
    OffscreenCanvas: globalThis.OffscreenCanvas,
  };

  const devices = [
    { lost: Promise.resolve({ reason: "unknown", message: "" }), createBuffer() {} },
    {
      lost: Promise.resolve({ reason: "unknown", message: "" }),
      createBuffer(descriptor) {
        this.lastInvalidBuffer = descriptor;
        return {};
      },
    },
  ];
  let requestDeviceIndex = 0;
  class CanvasContext {
    configure(descriptor) {
      if (descriptor.fail) throw new Error("configuration rejected");
      this.device = descriptor.device;
    }
  }
  class OffscreenCanvas {
    getContext(type) {
      return type === "webgpu" ? (this.context ??= new CanvasContext()) : null;
    }
  }
  class HTMLCanvasElement extends OffscreenCanvas {
    constructor(id) {
      super();
      this.id = id;
    }

    transferControlToOffscreen() {
      return new OffscreenCanvas();
    }
  }
  const adapter = {
    async requestDevice() {
      return devices[requestDeviceIndex++];
    },
  };
  const gpu = { async requestAdapter() { return adapter; } };
  Object.defineProperty(globalThis, "navigator", { configurable: true, value: { gpu } });
  globalThis.HTMLCanvasElement = HTMLCanvasElement;
  globalThis.OffscreenCanvas = OffscreenCanvas;

  try {
    installCaptureState();
    const capture = globalThis.__noonWebGpuDeviceCapture;
    const patchedAdapter = await gpu.requestAdapter();

    // The first device belongs to an unowned capability probe.
    const probe = new OffscreenCanvas();
    probe.getContext("webgpu").configure({ device: await patchedAdapter.requestDevice() });
    assert.equal(capture.ownerDeviceIndex, null);

    // A failed configure must not claim ownership.
    const scene = new HTMLCanvasElement("scene");
    const sceneCanvas = scene.transferControlToOffscreen();
    const sceneContext = sceneCanvas.getContext("webgpu");
    const ownerDevice = await patchedAdapter.requestDevice();
    assert.throws(() => sceneContext.configure({ device: ownerDevice, fail: true }), /rejected/);
    assert.equal(capture.ownerDeviceIndex, null);

    sceneContext.configure({ device: ownerDevice });
    assert.equal(capture.ownerDeviceIndex, 1);
    assert.deepEqual(capture.configuredCanvasIds[1], ["scene"]);

    // The validation operation is directed to the configured render owner.
    const owner = capture.devices[capture.ownerDeviceIndex];
    owner.createBuffer({ size: 4, usage: 0 });
    assert.deepEqual(owner.lastInvalidBuffer, { size: 4, usage: 0 });

    // Reconfiguration does not create or select a replacement device.
    sceneContext.configure({ device: owner });
    assert.equal(capture.devices.length, 2);
    assert.equal(capture.ownerDeviceIndex, 1);
  } finally {
    if (previous.navigator === undefined) delete globalThis.navigator;
    else Object.defineProperty(globalThis, "navigator", { configurable: true, value: previous.navigator });
    globalThis.HTMLCanvasElement = previous.HTMLCanvasElement;
    globalThis.OffscreenCanvas = previous.OffscreenCanvas;
    delete globalThis.__noonWebGpuDeviceCapture;
  }
});
