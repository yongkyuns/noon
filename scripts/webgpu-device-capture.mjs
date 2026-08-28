export async function installWebGpuDeviceCapture(page) {
  await page.addInitScript(() => {
    const state = {
      patched: false,
      patchError: null,
      devices: [],
      lost: [],
    };
    window.__noonWebGpuDeviceCapture = state;

    try {
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
  });
}

export function readWebGpuCapture(page) {
  return page.evaluate(() => ({
    patched: window.__noonWebGpuDeviceCapture?.patched ?? false,
    patchError: window.__noonWebGpuDeviceCapture?.patchError ?? null,
    deviceCount: window.__noonWebGpuDeviceCapture?.devices.length ?? 0,
    lost: window.__noonWebGpuDeviceCapture?.lost ?? [],
  }));
}
