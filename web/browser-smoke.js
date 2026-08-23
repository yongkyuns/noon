import init, { NoonCanvasPlayer, demoSceneJson } from "./pkg/noon_web.js";

const canvas = document.querySelector("#scene");
const state = {
  ready: false,
  error: null,
  revision: 0,
  frames: 0,
};

let player = null;

window.noonSmoke = {
  state,
  loadScene() {
    throw new Error("Noon browser smoke harness is not ready");
  },
  renderAt() {
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

function presentAt(timeSeconds) {
  const time = Number(timeSeconds);
  if (!Number.isFinite(time) || time < 0 || time >= 4.0) {
    throw new RangeError("smoke render time must be finite and in [0, 4)");
  }

  // PlaybackClock treats the first timestamp after reset as semantic time zero.
  // A second controlled timestamp therefore renders the requested semantic time
  // through the exact production renderFrame path without depending on RAF speed.
  player.resetClock();
  if (player.renderFrame(0)) {
    state.frames += 1;
  }
  if (player.renderFrame(time * 1000)) {
    state.frames += 1;
  }
  return metrics();
}

async function start() {
  await init();
  player = await NoonCanvasPlayer.create(canvas, demoSceneJson(), 4.0);
  player.resize(canvas.width, canvas.height);

  window.noonSmoke.loadScene = (sceneJson) => {
    if (typeof sceneJson !== "string") {
      throw new TypeError("sceneJson must be a string");
    }
    const incremental = player.reconcileScene(sceneJson);
    state.revision += 1;
    state.error = null;
    return {
      incremental,
      revision: state.revision,
      objectCount: player.objectCount(),
    };
  };
  window.noonSmoke.renderAt = presentAt;
  window.noonSmoke.metrics = metrics;
  state.ready = true;
}

start().catch((error) => {
  state.error = String(error);
  state.ready = true;
  console.error(error);
});
