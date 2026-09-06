import init, {
  AuthoringSceneCore,
  EngineScenePlayer,
  ExecutionCanvasRenderer,
} from "./pkg/noon_web.js";
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
  revision: 0,
  frames: 0,
};

let engine = null;
let renderer = null;
let incrementalTime = null;
let backingWidth = canvas.width;
let backingHeight = canvas.height;
let rendererCanvas = null;
let webglContextRecovery = null;

window.noonSmoke = {
  state,
  loadScene() {
    throw new Error("Noon browser smoke harness is not ready");
  },
  renderAt() {
    throw new Error("Noon browser smoke harness is not ready");
  },
  beginIncremental() {
    throw new Error("Noon browser smoke harness is not ready");
  },
  renderIncrementalAt() {
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
      revision: state.revision,
      frames: state.frames,
    };
  },
};

function metrics() {
  return {
    ready: state.ready,
    error: state.error,
    revision: state.revision,
    frames: state.frames,
    time: renderer?.time() ?? Number.NaN,
    objectCount: renderer?.objectCount() ?? 0,
    drawCalls: renderer?.lastDrawCalls() ?? 0,
    instances: renderer?.lastInstancesDrawn() ?? 0,
    uploadBytes: renderer?.lastBytesUploaded() ?? 0,
    geometryCacheMisses: renderer?.lastGeometryCacheMisses() ?? 0,
    rendererBackend: renderer?.rendererBackend() ?? null,
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

function applyDelta(json) {
  if (json === undefined || json === null) {
    return false;
  }
  const applied = renderer.applyDeltaJson(json);
  drainGpuDiagnostics();
  return applied;
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

function seekWithSnapshot(time) {
  engine.seekDeltaJson(time);
  applyDelta(engine.snapshotDeltaJson());
}

async function presentAt(timeSeconds) {
  const time = validateRenderTime(timeSeconds);
  flushPending();
  seekWithSnapshot(time);
  incrementalTime = null;
  const presented = presentPending();
  if (presented) {
    await waitForPaint();
  }
  return { ...metrics(), presented };
}

async function beginIncremental() {
  flushPending();
  seekWithSnapshot(0.0);
  const presented = presentPending();
  incrementalTime = 0.0;
  if (presented) {
    await waitForPaint();
  }
  return { ...metrics(), presented };
}

async function presentIncrementalAt(timeSeconds) {
  const time = validateRenderTime(timeSeconds);
  if (incrementalTime === null) {
    throw new Error("incremental smoke playback must begin with a presented beginIncremental() frame");
  }
  if (time < incrementalTime) {
    throw new RangeError("incremental smoke render time must not move backwards");
  }
  if (time === incrementalTime) {
    return { ...metrics(), presented: true };
  }

  flushPending();
  const delta = engine.seekDeltaJson(time);
  if (delta === undefined || delta === null) {
    applyDelta(engine.snapshotDeltaJson());
  } else {
    applyDelta(delta);
  }
  const presented = presentPending();
  if (presented) {
    incrementalTime = time;
    await waitForPaint();
  }
  return { ...metrics(), presented };
}

async function resizeBacking(width, height) {
  backingWidth = validateBackingDimension("backing width", width);
  backingHeight = validateBackingDimension("backing height", height);
  renderer.resize(backingWidth, backingHeight);
  incrementalTime = null;
  await waitForPaint();
  return metrics();
}

async function start() {
  await init();

  const bootstrap = new AuthoringSceneCore();
  engine = new EngineScenePlayer(
    bootstrap.sceneJson(),
    SMOKE_RENDER_HORIZON_SECONDS,
    1,
  );
  const offscreen = canvas.transferControlToOffscreen();
  rendererCanvas = offscreen;
  renderer = await ExecutionCanvasRenderer.create(offscreen, engine.initialDeltaJson());
  renderer.resize(backingWidth, backingHeight);
  renderer.setCamera(0.0, 0.0, MANIM_DEFAULT_CAMERA_HEIGHT);
  presentPending();

  window.noonSmoke.loadScene = (sceneJson) => {
    if (typeof sceneJson !== "string") {
      throw new TypeError("sceneJson must be a string");
    }
    flushPending();
    const result = JSON.parse(engine.reconcileSceneDeltaJson(sceneJson));
    applyDelta(result.delta);
    incrementalTime = null;
    state.revision += 1;
    state.error = null;
    return {
      incremental: result.incremental,
      revision: state.revision,
      objectCount: renderer.objectCount(),
    };
  };
  window.noonSmoke.renderAt = presentAt;
  window.noonSmoke.beginIncremental = beginIncremental;
  window.noonSmoke.renderIncrementalAt = presentIncrementalAt;
  window.noonSmoke.resizeBacking = resizeBacking;
  window.noonSmoke.webglContextControl = () => {
    if (webglContextRecovery !== null) return webglContextRecovery;
    const gl = rendererCanvas?.getContext("webgl2");
    const extension = gl?.getExtension("WEBGL_lose_context");
    if (!gl || !extension) return null;
    const state = { lost: 0, restored: 0 };
    rendererCanvas.addEventListener("webglcontextlost", (event) => {
      event.preventDefault();
      state.lost += 1;
    });
    rendererCanvas.addEventListener("webglcontextrestored", () => {
      state.restored += 1;
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
