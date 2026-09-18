export function installCaptureState() {
  const state = {
    patched: false,
    patchError: null,
    devices: [],
    lost: [],
    configuredCanvasIds: [],
    ownerDeviceIndex: null,
  };
  globalThis.__noonWebGpuDeviceCapture = state;

  try {
    const canvasOwners = new WeakMap();
    const configuredContexts = new WeakSet();
    const recordCanvasConfiguration = (canvas, device) => {
      const deviceIndex = state.devices.indexOf(device);
      if (deviceIndex < 0) return;
      const canvasId = canvasOwners.get(canvas) ?? canvas.id ?? null;
      state.configuredCanvasIds[deviceIndex] ??= [];
      if (!state.configuredCanvasIds[deviceIndex].includes(canvasId)) {
        state.configuredCanvasIds[deviceIndex].push(canvasId);
      }
      if (canvasId === "scene") state.ownerDeviceIndex = deviceIndex;
    };
    const patchCanvasPrototype = (prototype) => {
      if (!prototype || typeof prototype.getContext !== "function") return;
      const originalGetContext = prototype.getContext;
      Object.defineProperty(prototype, "getContext", {
        configurable: true,
        value: function patchedGetContext(type, ...args) {
          const canvas = this;
          const context = originalGetContext.call(this, type, ...args);
          if (type !== "webgpu" || context === null || configuredContexts.has(context)) return context;
          const originalConfigure = context.configure;
          if (typeof originalConfigure !== "function") return context;
          Object.defineProperty(context, "configure", {
            configurable: true,
            value: function patchedConfigure(descriptor) {
              const result = originalConfigure.call(this, descriptor);
              recordCanvasConfiguration(canvas, descriptor?.device);
              return result;
            },
          });
          configuredContexts.add(context);
          return context;
        },
      });
    };
    patchCanvasPrototype(globalThis.HTMLCanvasElement?.prototype);
    patchCanvasPrototype(globalThis.OffscreenCanvas?.prototype);
    if (globalThis.HTMLCanvasElement?.prototype?.transferControlToOffscreen) {
      const originalTransfer = globalThis.HTMLCanvasElement.prototype.transferControlToOffscreen;
      Object.defineProperty(globalThis.HTMLCanvasElement.prototype, "transferControlToOffscreen", {
        configurable: true,
        value: function patchedTransferControlToOffscreen(...args) {
          const offscreen = originalTransfer.apply(this, args);
          canvasOwners.set(offscreen, this.id ?? null);
          return offscreen;
        },
      });
    }
    const gpu = navigator.gpu;
    if (!gpu || typeof gpu.requestAdapter !== "function") {
      state.patchError = "navigator.gpu.requestAdapter is unavailable";
      return;
    }
    const originalRequestAdapter = gpu.requestAdapter.bind(gpu);
    Object.defineProperty(gpu, "requestAdapter", {
      configurable: true,
      value: async (...adapterArgs) => {
        const adapter = await originalRequestAdapter(...adapterArgs);
        if (!adapter) return adapter;

        const originalRequestDevice = adapter.requestDevice.bind(adapter);
        Object.defineProperty(adapter, "requestDevice", {
          configurable: true,
          value: async (...deviceArgs) => {
            const device = await originalRequestDevice(...deviceArgs);
            const index = state.devices.length;
            state.devices.push(device);
            state.lost.push(null);
            state.configuredCanvasIds.push([]);
            device.lost.then((info) => {
              state.lost[index] = {
                reason: String(info.reason ?? "unknown"),
                message: String(info.message ?? ""),
              };
            });
            return device;
          },
        });
        return adapter;
      },
    });
    state.patched = true;
  } catch (error) {
    state.patchError = String(error);
  }
}

function captureSnapshot() {
  return {
    patched: globalThis.__noonWebGpuDeviceCapture?.patched ?? false,
    patchError: globalThis.__noonWebGpuDeviceCapture?.patchError ?? null,
    deviceCount: globalThis.__noonWebGpuDeviceCapture?.devices.length ?? 0,
    lost: globalThis.__noonWebGpuDeviceCapture?.lost ?? [],
    configuredCanvasIds: globalThis.__noonWebGpuDeviceCapture?.configuredCanvasIds ?? [],
    ownerDeviceIndex: globalThis.__noonWebGpuDeviceCapture?.ownerDeviceIndex ?? null,
  };
}

export async function installWebGpuDeviceCapture(page) {
  await page.addInitScript(installCaptureState);
}

export function readWebGpuCapture(page) {
  return page.evaluate(captureSnapshot);
}

export function installWebGpuDeviceCaptureInWorker(worker) {
  return worker.evaluate(installCaptureState);
}

export function readWorkerWebGpuCapture(worker) {
  return worker.evaluate(captureSnapshot);
}
