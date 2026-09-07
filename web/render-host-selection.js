export const RENDER_HOST_WORKER = "worker";
export const RENDER_HOST_MAIN_THREAD = "main-thread";

let cachedAutomaticSelection = null;

// Return a host immediately when the answer is explicit or probing is unavailable.
// Browser capability probing remains asynchronous, which lets ExecutionWorkerClient
// preserve its historical synchronous startup contract in unit/non-browser realms.
export function selectExecutionRenderHost({ force = null } = {}) {
  const requested = force ?? requestedRenderHost();
  if (requested !== null) return validateRenderHost(requested);
  if (!canProbeBrowserRenderHosts()) return RENDER_HOST_WORKER;
  cachedAutomaticSelection ??= probeRenderHost();
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
  if (probeMainThreadGpuSurface()) return RENDER_HOST_MAIN_THREAD;
  throw new Error(
    "Noon could not initialize a GPU canvas surface in either a worker or the main thread",
  );
}

async function probeWorkerGpuSurface() {
  if (typeof HTMLCanvasElement.prototype.transferControlToOffscreen !== "function") {
    return false;
  }
  const source = `
    self.onmessage = (event) => {
      try {
        const canvas = event.data.canvas;
        let ok = false;
        try { ok = canvas.getContext("webgpu") !== null; } catch {}
        if (!ok) {
          try { ok = canvas.getContext("webgl2") !== null; } catch {}
        }
        self.postMessage({ ok, error: "" });
      } catch (error) {
        self.postMessage({ ok: false, error: String(error) });
      }
    };
  `;
  const workerUrl = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
  const worker = new Worker(workerUrl);
  try {
    const htmlCanvas = document.createElement("canvas");
    htmlCanvas.width = 2;
    htmlCanvas.height = 2;
    const offscreen = htmlCanvas.transferControlToOffscreen();
    const result = await new Promise((resolve) => {
      const timeout = setTimeout(() => resolve({ ok: false }), 3000);
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
    worker.terminate();
    URL.revokeObjectURL(workerUrl);
  }
}

function probeMainThreadGpuSurface() {
  if (typeof HTMLCanvasElement.prototype.transferControlToOffscreen !== "function") {
    return false;
  }
  try {
    const htmlCanvas = document.createElement("canvas");
    htmlCanvas.width = 2;
    htmlCanvas.height = 2;
    const surface = htmlCanvas.transferControlToOffscreen();
    try {
      if (surface.getContext("webgpu") !== null) return true;
    } catch {}
    try {
      return surface.getContext("webgl2") !== null;
    } catch {
      return false;
    }
  } catch {
    return false;
  }
}
