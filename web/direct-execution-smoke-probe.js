import {
  createDirectAffineCallbackSmokeRenderer,
  createDirectAffineCompletionSmokeRenderer,
  createDirectExecutionSmokeRenderer,
  createDirectValueTrackerSmokeRenderer,
} from "./pkg/noon_web.js";
import { createDirectExecutionWakeDriver } from "./direct-execution-wake-driver.js";

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

async function waitForDirectExecutionToSettle(renderer, driver) {
  for (let attempt = 0; attempt < 300; attempt += 1) {
    const stats = driver.stats();
    if (renderer.time() >= 0.1 && stats.idle && stats.presentedFrames > 0) {
      return stats;
    }
    await sleep(10);
  }
  throw new Error(
    `direct execution wake driver did not settle: time=${renderer.time()}, stats=${JSON.stringify(driver.stats())}`,
  );
}

async function presentDirectFrame(renderer) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    if (renderer.render()) {
      return;
    }
    await sleep(10);
  }
  throw new Error("direct affine callback renderer could not acquire a frame");
}

async function advanceDirectCallbackFrame(renderer, wallTimeMs) {
  const directive = JSON.parse(renderer.directWakeDirectiveJson(wallTimeMs));
  if (directive.cadence !== "animation-frame") {
    throw new Error(
      `direct affine callbacks expected runtime animation-frame cadence, got ${directive.cadence}`,
    );
  }
  if (!renderer.advanceDirectRealtime(wallTimeMs)) {
    throw new Error(`direct affine callbacks published no frame at ${wallTimeMs}ms`);
  }
  await presentDirectFrame(renderer);
}

async function sampleRenderedPixel(canvas, worldX, worldY) {
  const bitmap = await createImageBitmap(await canvas.convertToBlob({ type: "image/png" }));
  const pixels = new OffscreenCanvas(canvas.width, canvas.height);
  const context = pixels.getContext("2d", { willReadFrequently: true });
  if (!context) {
    throw new Error("direct Rust/WASM proof could not create its pixel reader");
  }
  context.drawImage(bitmap, 0, 0);
  bitmap.close();

  const worldHeight = 8;
  const worldWidth = worldHeight * (canvas.width / canvas.height);
  const x = Math.round(((worldX + worldWidth / 2) / worldWidth) * (canvas.width - 1));
  const y = Math.round(((worldHeight / 2 - worldY) / worldHeight) * (canvas.height - 1));
  const data = context.getImageData(x, y, 1, 1).data;
  return data[0] + data[1] + data[2];
}

async function sampleRenderedNeighborhood(canvas, worldX, worldY, radius = 4) {
  const bitmap = await createImageBitmap(await canvas.convertToBlob({ type: "image/png" }));
  const pixels = new OffscreenCanvas(canvas.width, canvas.height);
  const context = pixels.getContext("2d", { willReadFrequently: true });
  if (!context) {
    throw new Error("direct Rust/WASM proof could not create its pixel reader");
  }
  context.drawImage(bitmap, 0, 0);
  bitmap.close();

  const worldHeight = 8;
  const worldWidth = worldHeight * (canvas.width / canvas.height);
  const centerX = Math.round(((worldX + worldWidth / 2) / worldWidth) * (canvas.width - 1));
  const centerY = Math.round(((worldHeight / 2 - worldY) / worldHeight) * (canvas.height - 1));
  const x = Math.max(0, centerX - radius);
  const y = Math.max(0, centerY - radius);
  const width = Math.min(canvas.width - x, radius * 2 + 1);
  const height = Math.min(canvas.height - y, radius * 2 + 1);
  const data = context.getImageData(x, y, width, height).data;
  let brightest = 0;
  for (let offset = 0; offset < data.length; offset += 4) {
    brightest = Math.max(brightest, data[offset] + data[offset + 1] + data[offset + 2]);
  }
  return brightest;
}

async function directAffineCallbackProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectAffineCallbackSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow) {
    throw new Error("direct affine callback session did not expose its initial publication");
  }
  await presentDirectFrame(renderer);
  const bootstrapSourceLuma = await sampleRenderedPixel(canvas, 0, 1);
  const bootstrapDriftLuma = await sampleRenderedPixel(canvas, -3, 0);
  const bootstrapVacatedLuma = await sampleRenderedPixel(canvas, 0, -0.75);
  if (
    bootstrapSourceLuma < 180 ||
    bootstrapDriftLuma < bootstrapSourceLuma + 120 ||
    bootstrapVacatedLuma > 60
  ) {
    throw new Error(
      `direct affine callback bootstrap did not publish its time-zero phase: ${JSON.stringify({
        bootstrapSourceLuma,
        bootstrapDriftLuma,
        bootstrapVacatedLuma,
      })}`,
    );
  }
  await advanceDirectCallbackFrame(renderer, 1000);
  await advanceDirectCallbackFrame(renderer, 2000);

  const sourceLuma = await sampleRenderedPixel(canvas, 2, 1);
  const driftLuma = await sampleRenderedPixel(canvas, -3, 2);
  const backgroundLuma = await sampleRenderedPixel(canvas, 0, -3);
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    sourceLuma,
    driftLuma,
    backgroundLuma,
    bootstrapSourceLuma,
    bootstrapDriftLuma,
    bootstrapVacatedLuma,
  };
  if (metrics.backend !== expectedBackend) {
    throw new Error(
      `direct affine callback renderer selected ${metrics.backend}; expected ${expectedBackend}`,
    );
  }
  if (metrics.authoredTime !== 2) {
    throw new Error(`direct affine callback authored time is ${metrics.authoredTime}; expected 2`);
  }
  if (metrics.objectCount !== 2 || metrics.drawCalls <= 0) {
    throw new Error(`direct affine callback renderer produced invalid metrics ${JSON.stringify(metrics)}`);
  }
  if (
    sourceLuma < 180 ||
    driftLuma < 600 ||
    driftLuma < sourceLuma + 120 ||
    backgroundLuma > 60
  ) {
    throw new Error(`direct affine callback pixels do not match the Rust scene ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function directAffineCompletionProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectAffineCompletionSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow) {
    throw new Error("direct affine completion session did not expose its settled publication");
  }
  await presentDirectFrame(renderer);

  // The target-neutral Rust builder has already asserted the complete authored
  // and effective sequence. Pixels at the final top edge distinguish x=5 from
  // the intervening authored setter at x=3 without exporting scene state to JS.
  const endpointLuma = await sampleRenderedNeighborhood(canvas, 5, -1);
  const priorSetterLuma = await sampleRenderedNeighborhood(canvas, 3, -1);
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    endpointLuma,
    priorSetterLuma,
  };
  if (metrics.backend !== expectedBackend) {
    throw new Error(
      `direct affine completion renderer selected ${metrics.backend}; expected ${expectedBackend}`,
    );
  }
  if (metrics.authoredTime !== 4.25) {
    throw new Error(`direct affine completion authored time is ${metrics.authoredTime}; expected 4.25`);
  }
  if (metrics.objectCount !== 1 || metrics.drawCalls <= 0) {
    throw new Error(`direct affine completion produced invalid metrics ${JSON.stringify(metrics)}`);
  }
  if (endpointLuma < 150 || priorSetterLuma > 60) {
    throw new Error(`direct affine completion did not render its x=5 endpoint ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function directValueTrackerProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectValueTrackerSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow) {
    throw new Error("direct ValueTracker session did not expose its settled publication");
  }
  await presentDirectFrame(renderer);

  const endpointLuma = await sampleRenderedPixel(canvas, 2, 0);
  const midpointLuma = await sampleRenderedPixel(canvas, 0, 0);
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    endpointLuma,
    midpointLuma,
  };
  if (metrics.backend !== expectedBackend) {
    throw new Error(
      `direct ValueTracker renderer selected ${metrics.backend}; expected ${expectedBackend}`,
    );
  }
  if (metrics.authoredTime !== 2) {
    throw new Error(`direct ValueTracker authored time is ${metrics.authoredTime}; expected 2`);
  }
  if (metrics.objectCount !== 1 || metrics.drawCalls <= 0) {
    throw new Error(`direct ValueTracker produced invalid metrics ${JSON.stringify(metrics)}`);
  }
  if (endpointLuma < 600 || midpointLuma > 60) {
    throw new Error(`direct ValueTracker did not render its x=2 endpoint ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function start() {
  const expectedBackend = await waitForPrimaryRenderer();
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectExecutionSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);
  const driver = createDirectExecutionWakeDriver(renderer);

  let wakeStats;
  try {
    wakeStats = await waitForDirectExecutionToSettle(renderer, driver);
  } finally {
    driver.stop();
  }

  const staticFrameSkipped = renderer.render() === false;
  const metrics = {
    backend: renderer.rendererBackend(),
    presented: wakeStats.presentedFrames > 0,
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    textDrawCalls: renderer.lastTextDrawCalls(),
    bytesUploaded: renderer.lastBytesUploaded(),
    authoredTime: renderer.time(),
    scheduledAnimationFrames: wakeStats.scheduledAnimationFrames,
    scheduledTimers: wakeStats.scheduledTimers,
    idle: wakeStats.idle,
    staticFrameSkipped,
  };

  if (metrics.backend !== expectedBackend) {
    throw new Error(
      `direct execution renderer selected ${metrics.backend}; expected ${expectedBackend}`,
    );
  }
  if (!metrics.presented) {
    throw new Error("direct execution renderer did not present its semantic frame");
  }
  if (metrics.objectCount !== 3) {
    throw new Error(
      `direct execution renderer expected animated circle, camera, and text object, got ${metrics.objectCount}`,
    );
  }
  if (metrics.drawCalls <= 0) {
    throw new Error(`direct execution renderer emitted ${metrics.drawCalls} draw calls`);
  }
  if (metrics.textDrawCalls <= 0) {
    throw new Error("direct execution mixed renderer emitted no text draw calls");
  }
  if (metrics.drawCalls <= metrics.textDrawCalls) {
    throw new Error("direct execution mixed renderer emitted no geometry draw calls");
  }
  if (metrics.bytesUploaded <= 0) {
    throw new Error(`direct execution renderer uploaded ${metrics.bytesUploaded} bytes`);
  }
  if (metrics.authoredTime < 0.1) {
    throw new Error(`direct execution authored time stopped at ${metrics.authoredTime}`);
  }
  if (metrics.scheduledAnimationFrames <= 0) {
    throw new Error("direct execution wake driver never requested an animation frame");
  }

  if (!metrics.idle) {
    throw new Error("direct execution wake driver did not become idle after authored work settled");
  }
  if (!metrics.staticFrameSkipped) {
    throw new Error("direct execution renderer prepared a static frame without a publication");
  }

  metrics.affineCallbacks = await directAffineCallbackProof(expectedBackend);
  metrics.affineCompletion = await directAffineCompletionProof(expectedBackend);
  metrics.valueTracker = await directValueTrackerProof(expectedBackend);

  state.metrics = metrics;
  state.ready = true;
}

start().catch((error) => {
  state.error = String(error);
  state.ready = true;
  console.error(error);
});
