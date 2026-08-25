import init, { NoonCanvasPlayer, demoSceneJson } from "./pkg/noon_web.js";

const canvas = document.querySelector("#scene");
const MANIM_DEFAULT_CAMERA_HEIGHT = 8.0;
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
  };
}

function validateRenderTime(timeSeconds) {
  const time = Number(timeSeconds);
  if (!Number.isFinite(time) || time < 0 || time >= 4.0) {
    throw new RangeError("smoke render time must be finite and in [0, 4)");
  }
  return time;
}

function presentAt(timeSeconds) {
  const time = validateRenderTime(timeSeconds);

  // PlaybackClock treats the first timestamp after reset as semantic time zero.
  // A second controlled timestamp therefore renders the requested semantic time
  // through the exact production renderFrame path without depending on RAF speed.
  // At t=0 the origin frame is already the requested frame, so do not render it
  // twice; that would test renderer idempotence rather than seek/playback parity.
  player.resetClock();
  incrementalTime = null;
  let presented = player.renderFrame(0);
  if (presented) {
    state.frames += 1;
  }
  if (time > 0) {
    presented = player.renderFrame(time * 1000);
    if (presented) {
      state.frames += 1;
    }
  }
  return { ...metrics(), presented };
}

function beginIncremental() {
  player.resetClock();
  incrementalTime = null;
  const presented = player.renderFrame(0);
  if (presented) {
    state.frames += 1;
    incrementalTime = 0.0;
  }
  return { ...metrics(), presented };
}

function presentIncrementalAt(timeSeconds) {
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
  const presented = player.renderFrame(time * 1000);
  if (presented) {
    state.frames += 1;
    incrementalTime = time;
  }
  return { ...metrics(), presented };
}

async function start() {
  await init();
  player = await NoonCanvasPlayer.create(canvas, demoSceneJson(), 4.0);
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
  window.noonSmoke.metrics = metrics;
  state.ready = true;
}

start().catch((error) => {
  state.error = String(error);
  state.ready = true;
  console.error(error);
});