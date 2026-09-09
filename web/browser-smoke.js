import init, { createDirectRecoverySmokeRenderer } from "./pkg/noon_web.js";
import {
  drainRendererGpuDiagnostics,
  formatGpuDiagnostic,
} from "./render-gpu-diagnostics.js";

const canvas = document.querySelector("#scene");
const MANIM_DEFAULT_CAMERA_HEIGHT = 8.0;
const MAX_PRESENT_ATTEMPTS = 4;
const SMOKE_RENDER_HORIZON_SECONDS = 24 * 60 * 60;
const state = {
  ready: false,
  error: null,
  frames: 0,
};

let renderer = null;
let backingWidth = canvas.width;
let backingHeight = canvas.height;
let rendererCanvas = null;
let webglContextRecovery = null;

window.noonSmoke = {
  state,
  renderAt() {
    throw new Error("Noon browser smoke harness is not ready");
  },
  resizeBacking() {
    throw new Error("Noon browser smoke harness is not ready");
  },
  webglContextControl() {
    throw new Error("Noon browser smoke harness is not ready");
  },
  metrics() {
    return {
      ready: state.ready,
      error: state.error,
      revision: renderer ? renderer.directSceneRevision().toString() : null,
      frames: state.frames,
    };
  },
};

function metrics() {
  return {
    ready: state.ready,
    error: state.error,
    revision: renderer ? renderer.directSceneRevision().toString() : null,
    frames: state.frames,
    time: renderer?.time() ?? Number.NaN,
    objectCount: renderer?.objectCount() ?? 0,
    drawCalls: renderer?.lastDrawCalls() ?? 0,
    instances: renderer?.lastInstancesDrawn() ?? 0,
    uploadBytes: renderer?.lastBytesUploaded() ?? 0,
    geometryCacheMisses: renderer?.lastGeometryCacheMisses() ?? 0,
    rendererBackend: renderer?.rendererBackend() ?? null,
    gpuGeneration: renderer?.gpuGeneration() ?? null,
    backingWidth,
    backingHeight,
    cssWidth: canvas.clientWidth,
    cssHeight: canvas.clientHeight,
  };
}

function validateRenderTime(timeSeconds) {
  const time = Number(timeSeconds);
  if (!Number.isFinite(time) || time < 0 || time >= SMOKE_RENDER_HORIZON_SECONDS) {
    throw new RangeError(
      `smoke render time must be finite and in [0, ${SMOKE_RENDER_HORIZON_SECONDS})`,
    );
  }
  return time;
}

function validateBackingDimension(name, value) {
  const dimension = Number(value);
  if (!Number.isSafeInteger(dimension) || dimension <= 0) {
    throw new RangeError(`${name} must be a positive integer`);
  }
  return dimension;
}

function drainGpuDiagnostics() {
  let fatal = null;
  const healthy = drainRendererGpuDiagnostics(renderer, {
    onRecoverable(diagnostic) {
      console.warn(formatGpuDiagnostic(diagnostic));
    },
    onFatal(diagnostic) {
      fatal = new Error(formatGpuDiagnostic(diagnostic));
    },
  });
  if (!healthy) {
    throw fatal ?? new Error("renderer reported a fatal GPU diagnostic");
  }
}

function recordPresent() {
  drainGpuDiagnostics();
  const presented = renderer.render();
  drainGpuDiagnostics();
  if (presented) {
    state.frames += 1;
  }
  return presented;
}

function presentPending() {
  let presented = false;
  for (let attempt = 0; attempt < MAX_PRESENT_ATTEMPTS && !presented; attempt += 1) {
    presented = recordPresent();
  }
  return presented;
}

function waitForPaint() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(resolve));
  });
}

function flushPending() {
  presentPending();
}

async function presentAt(timeSeconds) {
  const time = validateRenderTime(timeSeconds);
  flushPending();
  renderer.seekDirect(time);
  // These diagnostic calls explicitly request a fresh platform frame at the
  // same authored time, including repeated GPU error/recovery probes.
  renderer.setCamera(0.0, 0.0, MANIM_DEFAULT_CAMERA_HEIGHT);
  const presented = presentPending();
  if (presented) await waitForPaint();
  return { ...metrics(), presented };
}

async function resizeBacking(width, height) {
  backingWidth = validateBackingDimension("backing width", width);
  backingHeight = validateBackingDimension("backing height", height);
  renderer.resize(backingWidth, backingHeight);
  const directive = JSON.parse(renderer.directWakeDirectiveJson(performance.now()));
  const presented = directive.presentNow && presentPending();
  await waitForPaint();
  return { ...metrics(), presented };
}

async function start() {
  await init();

  rendererCanvas = canvas.transferControlToOffscreen();
  const fixture = new URL(location.href).searchParams.get("fixture") ?? "circle";
  renderer = await createDirectRecoverySmokeRenderer(rendererCanvas, fixture);
  renderer.resize(backingWidth, backingHeight);
  presentPending();

  window.noonSmoke.renderAt = presentAt;
  window.noonSmoke.resizeBacking = resizeBacking;
  window.noonSmoke.webglContextControl = () => {
    if (webglContextRecovery !== null) return webglContextRecovery;
    const gl = rendererCanvas?.getContext("webgl2");
    const extension = gl?.getExtension("WEBGL_lose_context");
    if (!gl || !extension) return null;
    const state = { lost: 0, restored: 0, recovery: "idle" };
    rendererCanvas.addEventListener("webglcontextlost", (event) => {
      event.preventDefault();
      state.lost += 1;
    });
    rendererCanvas.addEventListener("webglcontextrestored", async () => {
      state.restored += 1;
      state.recovery = "pending";
      try {
        await renderer.recoverWebGlContext();
        state.recovery = "ready";
      } catch (error) {
        state.recovery = "error";
        state.error = String(error?.message ?? error);
      }
    });
    webglContextRecovery = {
      state,
      lose: () => extension.loseContext(),
      restore: () => extension.restoreContext(),
    };
    return webglContextRecovery;
  };
  window.noonSmoke.metrics = metrics;
  state.ready = true;
}

start().catch((error) => {
  state.error = String(error);
  state.ready = true;
  console.error(error);
});
