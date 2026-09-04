import { createDirectExecutionSmokeRenderer } from "./pkg/noon_web.js";

const state = {
  ready: false,
  error: null,
  metrics: null,
};
window.noonDirectExecutionSmoke = state;

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function waitForPrimaryRenderer() {
  for (let attempt = 0; attempt < 3000; attempt += 1) {
    const primary = window.noonSmoke;
    if (primary?.state.ready === true) {
      if (primary.state.error) {
        throw new Error(`primary browser smoke failed: ${primary.state.error}`);
      }
      return primary.metrics().rendererBackend;
    }
    await sleep(10);
  }
  throw new Error("primary browser smoke did not initialize before direct execution proof");
}

async function start() {
  const expectedBackend = await waitForPrimaryRenderer();
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectExecutionSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  let presented = false;
  for (let attempt = 0; attempt < 4 && !presented; attempt += 1) {
    presented = renderer.render();
  }

  const metrics = {
    backend: renderer.rendererBackend(),
    presented,
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    bytesUploaded: renderer.lastBytesUploaded(),
  };

  if (metrics.backend !== expectedBackend) {
    throw new Error(
      `direct execution renderer selected ${metrics.backend}; expected ${expectedBackend}`,
    );
  }
  if (!metrics.presented) {
    throw new Error("direct execution renderer did not present its semantic frame");
  }
  if (metrics.objectCount !== 2) {
    throw new Error(
      `direct execution renderer expected semantic object plus camera frame (2 objects), got ${metrics.objectCount}`,
    );
  }
  if (metrics.drawCalls <= 0) {
    throw new Error(`direct execution renderer emitted ${metrics.drawCalls} draw calls`);
  }
  if (metrics.bytesUploaded <= 0) {
    throw new Error(`direct execution renderer uploaded ${metrics.bytesUploaded} bytes`);
  }

  state.metrics = metrics;
  state.ready = true;
}

start().catch((error) => {
  state.error = String(error);
  state.ready = true;
  console.error(error);
  setTimeout(() => {
    throw error;
  }, 0);
});
