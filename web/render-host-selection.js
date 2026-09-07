export const RENDER_HOST_WORKER = "worker";
export const RENDER_HOST_MAIN_THREAD = "main-thread";

let cachedAutomaticSelection = null;

// A WebGPU canvas context alone does not establish adapter availability.
// Check the adapter before claiming the canvas, then use wgpu-hal's WebGL
// surface settings when WebGPU is unavailable.
async function probeGpuSurface(canvas, gpu) {
  let context = null;
  if (typeof gpu?.requestAdapter === "function") {
    try {
      const adapter = await gpu.requestAdapter({
        powerPreference: "high-performance",
        forceFallbackAdapter: false,
      });
      if (adapter !== null) context = canvas.getContext("webgpu");
    } catch {}
  }
  if (context !== null) return context;
  try {
    return canvas.getContext("webgl2", { antialias: false });
  } catch {
    return null;
  }
}

// Return a host immediately when the answer is explicit or probing is unavailable.
// Browser capability probing remains asynchronous, which lets ExecutionWorkerClient
// preserve its historical synchronous startup contract in unit/non-browser realms.
export function selectExecutionRenderHost({ force = null } = {}) {
  const requested = force ?? requestedRenderHost();
  if (requested !== null) return validateRenderHost(requested);
  if (!canProbeBrowserRenderHosts()) return RENDER_HOST_WORKER;
  cachedAutomaticSelection ??= probeRenderHost().catch((error) => {
    cachedAutomaticSelection = null;
    throw error;
  });
  return cachedAutomaticSelection;
}

export function resetRenderHostSelectionForTests() {
  cachedAutomaticSelection = null;
}

function requestedRenderHost() {
  const explicit = globalThis.__NOON_RENDER_HOST__;
  if (explicit !== undefined && explicit !== null && explicit !== "") {
    return explicit;
  }
  try {
    const href = globalThis.location?.href;
    if (typeof href !== "string") return null;
    return new URL(href).searchParams.get("renderHost");
  } catch {
    return null;
  }
}

function validateRenderHost(value) {
  if (value !== RENDER_HOST_WORKER && value !== RENDER_HOST_MAIN_THREAD) {
    throw new Error(`unsupported Noon render host ${value}`);
  }
  return value;
}

function canProbeBrowserRenderHosts() {
  return (
    typeof globalThis.HTMLCanvasElement === "function" &&
    typeof globalThis.document?.createElement === "function" &&
    typeof globalThis.Worker === "function" &&
    typeof globalThis.Blob === "function" &&
    typeof globalThis.URL?.createObjectURL === "function"
  );
}

async function probeRenderHost() {
  if (await probeWorkerGpuSurface()) return RENDER_HOST_WORKER;
  if (await probeMainThreadGpuSurface()) return RENDER_HOST_MAIN_THREAD;
  throw new Error(
    "Noon could not initialize a GPU canvas surface in either a worker or the main thread",
  );
}

async function probeWorkerGpuSurface() {
  if (typeof HTMLCanvasElement.prototype.transferControlToOffscreen !== "function") {
    return false;
  }
  const source = `
    const probeGpuSurface = ${probeGpuSurface.toString()};
    self.onmessage = async (event) => {
      try {
        const canvas = event.data.canvas;
        const context = await probeGpuSurface(canvas, self.navigator?.gpu);
        const ok = context !== null;
        context?.unconfigure?.();
        context?.getExtension?.("WEBGL_lose_context")?.loseContext();
        self.postMessage({ ok, error: "" });
      } catch (error) {
        self.postMessage({ ok: false, error: String(error) });
      }
    };
  `;
  let workerUrl = null;
  let worker = null;
  let timeout = null;
  let htmlCanvas = null;
  try {
    workerUrl = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
    worker = new Worker(`${workerUrl}#noon-render-capability-probe`, {
      type: "module",
      name: "noon-render-capability-probe",
    });
    htmlCanvas = document.createElement("canvas");
    htmlCanvas.width = 2;
    htmlCanvas.height = 2;
    htmlCanvas.hidden = true;
    document.body?.append(htmlCanvas);
    const offscreen = htmlCanvas.transferControlToOffscreen();
    const result = await new Promise((resolve) => {
      timeout = setTimeout(() => resolve({ ok: false }), 3000);
      worker.onmessage = (event) => {
        clearTimeout(timeout);
        resolve(event.data);
      };
      worker.onerror = () => {
        clearTimeout(timeout);
        resolve({ ok: false });
      };
      worker.postMessage({ canvas: offscreen }, [offscreen]);
    });
    return result?.ok === true;
  } catch {
    return false;
  } finally {
    clearTimeout(timeout);
    worker?.terminate();
    htmlCanvas?.remove();
    if (workerUrl !== null) URL.revokeObjectURL(workerUrl);
  }
}

async function probeMainThreadGpuSurface() {
  if (typeof HTMLCanvasElement.prototype.transferControlToOffscreen !== "function") {
    return false;
  }
  let htmlCanvas = null;
  try {
    htmlCanvas = document.createElement("canvas");
    htmlCanvas.width = 2;
    htmlCanvas.height = 2;
    htmlCanvas.hidden = true;
    document.body?.append(htmlCanvas);
    const surface = htmlCanvas.transferControlToOffscreen();
    let context = null;
    try {
      context = await probeGpuSurface(surface, globalThis.navigator?.gpu);
      if (context === null) return false;
      return true;
    } catch {
      return false;
    } finally {
      context?.unconfigure?.();
      context?.getExtension?.("WEBGL_lose_context")?.loseContext();
    }
  } catch {
    return false;
  } finally {
    htmlCanvas?.remove();
  }
}
