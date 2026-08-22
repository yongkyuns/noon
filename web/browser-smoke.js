import init, { NoonCanvasPlayer, demoSceneJson } from "./pkg/noon_web.js";

const canvas = document.querySelector("#scene");
const state = {
  ready: false,
  error: null,
  revision: 0,
  frames: 0,
  framesSinceLoad: 0,
};

let player = null;

window.noonSmoke = {
  state,
  loadScene() {
    throw new Error("Noon browser smoke harness is not ready");
  },
  metrics() {
    return {
      ready: state.ready,
      error: state.error,
      revision: state.revision,
      frames: state.frames,
      framesSinceLoad: state.framesSinceLoad,
    };
  },
};

function metrics() {
  return {
    ready: state.ready,
    error: state.error,
    revision: state.revision,
    frames: state.frames,
    framesSinceLoad: state.framesSinceLoad,
    time: player?.time() ?? Number.NaN,
    objectCount: player?.objectCount() ?? 0,
    drawCalls: player?.lastDrawCalls() ?? 0,
    instances: player?.lastInstancesDrawn() ?? 0,
    uploadBytes: player?.lastBytesUploaded() ?? 0,
    geometryCacheMisses: player?.lastGeometryCacheMisses() ?? 0,
  };
}

async function start() {
  if (!navigator.gpu) {
    throw new Error("This browser does not expose WebGPU");
  }

  await init();
  player = await NoonCanvasPlayer.create(canvas, demoSceneJson(), 4.0);
  player.resize(canvas.width, canvas.height);

  window.noonSmoke.loadScene = (sceneJson) => {
    if (typeof sceneJson !== "string") {
      throw new TypeError("sceneJson must be a string");
    }
    const incremental = player.reconcileScene(sceneJson);
    player.resetClock();
    state.revision += 1;
    state.framesSinceLoad = 0;
    state.error = null;
    return {
      incremental,
      revision: state.revision,
      objectCount: player.objectCount(),
    };
  };
  window.noonSmoke.metrics = metrics;
  state.ready = true;

  function frame(timestamp) {
    try {
      if (player.renderFrame(timestamp)) {
        state.frames += 1;
        state.framesSinceLoad += 1;
      }
      requestAnimationFrame(frame);
    } catch (error) {
      state.error = String(error);
      console.error(error);
    }
  }

  requestAnimationFrame(frame);
}

start().catch((error) => {
  state.error = String(error);
  state.ready = true;
  console.error(error);
});
