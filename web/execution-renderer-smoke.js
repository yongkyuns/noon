import init, {
  EngineScenePlayer,
  ExecutionCanvasRenderer,
  demoSceneJson,
} from "./pkg/noon_web.js";

const state = {
  ready: false,
  error: null,
  metrics: null,
};

window.noonExecutionRendererSmoke = state;

async function start() {
  await init();
  const engine = new EngineScenePlayer(demoSceneJson(), 4.0, 1);
  const initialDelta = engine.initialDeltaJson();
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await ExecutionCanvasRenderer.create(canvas, initialDelta);
  renderer.resize(canvas.width, canvas.height);

  let presented = false;
  for (let attempt = 0; attempt < 4 && !presented; attempt += 1) {
    presented = renderer.render();
  }

  state.metrics = {
    rendererBackend: renderer.rendererBackend(),
    presented,
    drawCalls: renderer.lastDrawCalls(),
    objectCount: renderer.objectCount(),
  };
  state.ready = true;
}

start().catch((error) => {
  state.error = String(error);
  state.ready = true;
  console.error(error);
});
