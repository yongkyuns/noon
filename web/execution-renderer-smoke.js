import init, { createDirectRecoverySmokeRenderer } from "./pkg/noon_web.js";

const state = {
  ready: false,
  error: null,
  metrics: null,
};

window.noonExecutionRendererSmoke = state;

async function start() {
  await init();
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectRecoverySmokeRenderer(canvas, "circle");
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
