import init, { NoonCanvasPlayer, demoSceneJson } from "./pkg/noon_web.js";

const canvas = document.querySelector("#scene");
const MANIM_DEFAULT_CAMERA_HEIGHT = 8.0;
const MAX_PRESENT_ATTEMPTS = 4;
// The browser smoke harness is used by exact-output oracles as well as the
// four-second interactive playground. Do not encode the playground UX loop as a
// semantic rendering limit: parity fixtures must be sampled at their real Manim
// timestamps (for example Rotating's five-second default). This generous finite
// horizon keeps the production PlaybackClock/renderFrame path while leaving scene
// duration validation to the calling harness/fixture.
const SMOKE_RENDER_HORIZON_SECONDS = 24 * 60 * 60;
const state = {
  ready: false,
  error: null,
  revision: 0,
  frames: 0,
};

let player = null;
let incrementalTime = null;

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
    time: player?.time() ?? Number.NaN,
    objectCount: player?.objectCount() ?? 0,
    drawCalls: player?.lastDrawCalls() ?? 0,
    instances: player?.lastInstancesDrawn() ?? 0,
    uploadBytes: player?.lastBytesUploaded() ?? 0,
    geometryCacheMisses: player?.lastGeometryCacheMisses() ?? 0,
    rendererBackend: player?.rendererBackend() ?? null,
    backingWidth: canvas.width,
    backingHeight: canvas.height,
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

function recordPresent(timestampMs) {
  const presented = player.renderFrame(timestampMs);
  if (presented) {
    state.frames += 1;
  }
  return presented;
}

function presentTimestamp(timestampMs) {
  let presented = false;
  for (let attempt = 0; attempt < MAX_PRESENT_ATTEMPTS && !presented; attempt += 1) {
    presented = recordPresent(timestampMs);
  }
  return presented;
}

function waitForPaint() {
  // requestAnimationFrame callbacks run before their frame is painted. Crossing
  // two callbacks guarantees that the first callback's frame has completed its
  // paint/compositor handoff before Playwright reads the canvas.
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(resolve));
  });
}

async function presentAt(timeSeconds) {
  const time = validateRenderTime(timeSeconds);

  // PlaybackClock treats the first timestamp after reset as semantic time zero.
  // A second controlled timestamp therefore renders the requested semantic time
  // through the exact production renderFrame path without depending on RAF speed.
  // At t=0 the origin frame is already the requested frame, so do not render it
  // twice; that would test renderer idempotence rather than seek/playback parity.
  player.resetClock();
  incrementalTime = null;
  let presented = presentTimestamp(0);
  if (time > 0) {
    presented = presentTimestamp(time * 1000);
  }
  if (presented) {
    await waitForPaint();
  }
  return { ...metrics(), presented };
}

async function beginIncremental() {
  player.resetClock();
  incrementalTime = null;
  const presented = recordPresent(0);
  if (presented) {
    incrementalTime = 0.0;
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
  const presented = recordPresent(time * 1000);
  if (presented) {
    incrementalTime = time;
    await waitForPaint();
  }
  return { ...metrics(), presented };
}

async function resizeBacking(width, height) {
  const backingWidth = validateBackingDimension("backing width", width);
  const backingHeight = validateBackingDimension("backing height", height);
  canvas.width = backingWidth;
  canvas.height = backingHeight;
  player.resize(backingWidth, backingHeight);
  incrementalTime = null;
  await waitForPaint();
  return metrics();
}

async function start() {
  await init();
  player = await NoonCanvasPlayer.create(
    canvas,
    demoSceneJson(),
    SMOKE_RENDER_HORIZON_SECONDS,
  );
  player.resize(canvas.width, canvas.height);
  player.setCamera(0.0, 0.0, MANIM_DEFAULT_CAMERA_HEIGHT);

  window.noonSmoke.loadScene = (sceneJson) => {
    if (typeof sceneJson !== "string") {
      throw new TypeError("sceneJson must be a string");
    }
    const incremental = player.reconcileScene(sceneJson);
    incrementalTime = null;
    state.revision += 1;
    state.error = null;
    return {
      incremental,
      revision: state.revision,
      objectCount: player.objectCount(),
    };
  };
  window.noonSmoke.renderAt = presentAt;
  window.noonSmoke.beginIncremental = beginIncremental;
  window.noonSmoke.renderIncrementalAt = presentIncrementalAt;
  window.noonSmoke.resizeBacking = resizeBacking;
  window.noonSmoke.metrics = metrics;
  state.ready = true;
}

start().catch((error) => {
  state.error = String(error);
  state.ready = true;
  console.error(error);
});
