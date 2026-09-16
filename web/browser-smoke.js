import init, {
  createDirectRecoverySmokeRenderer,
  createDirectTransformMatchingShapesSmokeRenderer,
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
  frames: 0,
};

let renderer = null;
let backingWidth = canvas.width;
let backingHeight = canvas.height;
let rendererCanvas = null;
let webglContextRecovery = null;
let matchingShapesQualification = null;

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
    matchingShapesQualification,
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

async function recoverWebGpuIfNeeded() {
  const recovered = await renderer.recoverWebGpuDevice();
  if (recovered) {
    drainGpuDiagnostics();
  }
  return recovered;
}

async function presentAt(timeSeconds) {
  const time = validateRenderTime(timeSeconds);
  await recoverWebGpuIfNeeded();
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
  await recoverWebGpuIfNeeded();
  renderer.resize(backingWidth, backingHeight);
  const directive = JSON.parse(renderer.directWakeDirectiveJson(performance.now()));
  const presented = directive.presentNow && presentPending();
  await waitForPaint();
  return { ...metrics(), presented };
}

async function presentQualificationFrame(qualificationRenderer) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    if (qualificationRenderer.render()) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error("matching-shapes qualification could not acquire a frame");
}

async function settleQualification(qualificationRenderer, wallTimeMs) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    const directive = JSON.parse(
      qualificationRenderer.directWakeDirectiveJson(wallTimeMs),
    );
    if (!directive.presentNow) return directive;
    await presentQualificationFrame(qualificationRenderer);
  }
  throw new Error("matching-shapes qualification did not settle its publications");
}

async function renderedPixelContext(qualificationCanvas) {
  const bitmap = await createImageBitmap(
    await qualificationCanvas.convertToBlob({ type: "image/png" }),
  );
  const pixels = new OffscreenCanvas(
    qualificationCanvas.width,
    qualificationCanvas.height,
  );
  const context = pixels.getContext("2d", { willReadFrequently: true });
  if (!context) {
    bitmap.close();
    throw new Error("matching-shapes qualification could not create a pixel reader");
  }
  context.drawImage(bitmap, 0, 0);
  bitmap.close();
  return context;
}

async function sampleQualificationColor(qualificationCanvas, worldX, worldY) {
  const context = await renderedPixelContext(qualificationCanvas);
  const worldHeight = MANIM_DEFAULT_CAMERA_HEIGHT;
  const worldWidth = worldHeight * (qualificationCanvas.width / qualificationCanvas.height);
  const x = Math.round(
    ((worldX + worldWidth / 2) / worldWidth) * (qualificationCanvas.width - 1),
  );
  const y = Math.round(
    ((worldHeight / 2 - worldY) / worldHeight) *
      (qualificationCanvas.height - 1),
  );
  const data = context.getImageData(x, y, 1, 1).data;
  return { red: data[0], green: data[1], blue: data[2], alpha: data[3] };
}

async function qualificationYellowPixelCount(qualificationCanvas) {
  const context = await renderedPixelContext(qualificationCanvas);
  const data = context.getImageData(
    0,
    0,
    qualificationCanvas.width,
    qualificationCanvas.height,
  ).data;
  let count = 0;
  for (let index = 0; index < data.length; index += 4) {
    const red = data[index];
    const green = data[index + 1];
    const blue = data[index + 2];
    if (red > blue + 80 && green > blue + 80) count += 1;
  }
  return count;
}

function isPink(pixel) {
  return pixel.red > pixel.green + 50 && pixel.blue > pixel.green + 50;
}

function isBlue(pixel) {
  return pixel.blue > pixel.red + 70 && pixel.green > pixel.red + 50;
}

async function qualifyTransformMatchingShapes(expectedBackend) {
  const qualificationCanvas = new OffscreenCanvas(960, 540);
  const qualificationRenderer =
    await createDirectTransformMatchingShapesSmokeRenderer(qualificationCanvas);
  try {
    qualificationRenderer.resize(qualificationCanvas.width, qualificationCanvas.height);
    await settleQualification(qualificationRenderer, 0);

    qualificationRenderer.advanceDirectRealtime(500);
    await settleQualification(qualificationRenderer, 500);
    const midpointTriangle = await sampleQualificationColor(qualificationCanvas, 1, 0);
    const midpointKite = await sampleQualificationColor(qualificationCanvas, -1, 0);
    if (!isPink(midpointTriangle) || !isBlue(midpointKite)) {
      throw new Error(
        `matching-shapes midpoint used positional/index pairing: ${JSON.stringify({ midpointTriangle, midpointKite })}`,
      );
    }

    qualificationRenderer.advanceDirectRealtime(1000);
    await settleQualification(qualificationRenderer, 1000);
    const completedKite = await sampleQualificationColor(qualificationCanvas, -4, 0);
    const completedTriangle = await sampleQualificationColor(qualificationCanvas, 4, 0);
    if (!isBlue(completedKite) || !isPink(completedTriangle)) {
      throw new Error(
        `matching-shapes completion did not publish the authored target: ${JSON.stringify({ completedKite, completedTriangle })}`,
      );
    }

    qualificationRenderer.advanceDirectRealtime(1500);
    await settleQualification(qualificationRenderer, 1500);
    const yellowPixels = await qualificationYellowPixelCount(qualificationCanvas);
    if (yellowPixels < 500) {
      throw new Error(
        `matching-shapes replacement target did not survive into Indicate: yellowPixels=${yellowPixels}`,
      );
    }

    qualificationRenderer.advanceDirectRealtime(2000);
    const finalWake = await settleQualification(qualificationRenderer, 2000);
    const restoredKite = await sampleQualificationColor(qualificationCanvas, -4, 0);
    const restoredTriangle = await sampleQualificationColor(qualificationCanvas, 4, 0);
    if (!isBlue(restoredKite) || !isPink(restoredTriangle)) {
      throw new Error(
        `matching-shapes Indicate did not restore target paint: ${JSON.stringify({ restoredKite, restoredTriangle })}`,
      );
    }

    const result = {
      backend: qualificationRenderer.rendererBackend(),
      time: qualificationRenderer.time(),
      objectCount: qualificationRenderer.objectCount(),
      cadence: finalWake.cadence,
      midpointTriangle,
      midpointKite,
      completedKite,
      completedTriangle,
      yellowPixels,
      restoredKite,
      restoredTriangle,
    };
    if (
      result.backend !== expectedBackend ||
      result.time !== 2 ||
      result.objectCount !== 2 ||
      result.cadence !== "idle"
    ) {
      throw new Error(
        `matching-shapes qualification did not settle coherently: ${JSON.stringify(result)}`,
      );
    }
    return result;
  } finally {
    qualificationRenderer.free();
    if (expectedBackend === "WebGL2") {
      qualificationCanvas
        .getContext("webgl2")
        ?.getExtension("WEBGL_lose_context")
        ?.loseContext();
    }
  }
}

async function start() {
  await init();

  rendererCanvas = canvas.transferControlToOffscreen();
  const fixture = new URL(location.href).searchParams.get("fixture") ?? "circle";
  renderer = await createDirectRecoverySmokeRenderer(rendererCanvas, fixture);
  renderer.resize(backingWidth, backingHeight);
  presentPending();
  matchingShapesQualification = await qualifyTransformMatchingShapes(
    renderer.rendererBackend(),
  );

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
