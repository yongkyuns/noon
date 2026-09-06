const {
  createDirectAffineCallbackSmokeRenderer,
  createDirectCallbackPaintSmokeRenderer,
  createDirectLineMatchSmokeRenderer,
  createDirectAffineCompletionSmokeRenderer,
  createDirectExecutionSmokeRenderer,
  createDirectNativeSignalsSmokeRenderer,
  createDirectOrdinaryAffineCallbackContinuationSmokeRenderer,
  createDirectOrdinaryAffineContinuationSmokeRenderer,
  createDirectOrdinaryAffinePlaySmokeRenderer,
  createDirectOrdinaryCallbackSparseReadsSmokeRenderer,
  createDirectOrdinaryCompositionContinuationSmokeRenderer,
  createDirectOrdinaryValueTrackerContinuationSmokeRenderer,
  createDirectOrdinaryCompositionPlaySmokeRenderer,
  createDirectOrdinaryFadePlaySmokeRenderer,
  createDirectOrdinaryPaintPlaySmokeRenderer,
  createDirectOrdinaryStylePlaySmokeRenderer,
} = await import("./pkg/noon_web.js");
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

async function settleDirectPublication(renderer, wallTimeMs) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    await presentDirectFrame(renderer);
    // Presentation may resume Rust source and publish a final edit or begin its
    // next segment. Mirror the production driver's fresh wake observation.
    const directive = JSON.parse(renderer.directWakeDirectiveJson(wallTimeMs));
    if (!directive.presentNow) return directive;
  }
  throw new Error("direct source did not settle its presentation publications");
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
  // Mirror the production driver's post-callback wake observation. The test
  // clock assigns zero callback latency; the next timestamp advances from here.
  renderer.directWakeDirectiveJson(wallTimeMs);
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

async function sampleRenderedColor(canvas, worldX, worldY) {
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
  const [red, green, blue, alpha] = context.getImageData(x, y, 1, 1).data;
  return { red, green, blue, alpha };
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
    textDrawCalls: renderer.lastTextDrawCalls(),
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
  if (metrics.objectCount !== 3 || metrics.drawCalls <= 0 || metrics.textDrawCalls <= 0) {
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

async function directOrdinaryAffinePlayProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectOrdinaryAffinePlaySmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow) {
    throw new Error("direct ordinary affine session did not expose its settled publication");
  }
  await presentDirectFrame(renderer);

  // The shared Rust builder asserts each coherent center/time barrier. The
  // browser checks that the exact returned runtime presents only its final x=5
  // endpoint, rather than the first endpoint or intervening authored shift.
  const endpointLuma = await sampleRenderedNeighborhood(canvas, 5, -1);
  const firstEndpointLuma = await sampleRenderedNeighborhood(canvas, 2, -1);
  const shiftedLuma = await sampleRenderedNeighborhood(canvas, 3, -1);
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    endpointLuma,
    firstEndpointLuma,
    shiftedLuma,
  };
  if (metrics.backend !== expectedBackend) {
    throw new Error(
      `direct ordinary affine renderer selected ${metrics.backend}; expected ${expectedBackend}`,
    );
  }
  if (metrics.authoredTime !== 4) {
    throw new Error(`direct ordinary affine authored time is ${metrics.authoredTime}; expected 4`);
  }
  if (metrics.objectCount !== 1 || metrics.drawCalls <= 0) {
    throw new Error(`direct ordinary affine produced invalid metrics ${JSON.stringify(metrics)}`);
  }
  if (endpointLuma < 250 || firstEndpointLuma > 60 || shiftedLuma > 60) {
    throw new Error(`direct ordinary affine did not render its x=5 endpoint ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function directOrdinaryAffineContinuationProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectOrdinaryAffineContinuationSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow || initial.cadence !== "animation-frame") {
    throw new Error(`direct continuation did not start its shared animation: ${JSON.stringify(initial)}`);
  }
  await presentDirectFrame(renderer);

  if (!renderer.advanceDirectRealtime(1000)) {
    throw new Error("direct continuation did not publish its first midpoint");
  }
  await presentDirectFrame(renderer);
  const firstMidpointLuma = await sampleRenderedNeighborhood(canvas, 1, -0.5);

  if (!renderer.advanceDirectRealtime(2000)) {
    throw new Error("direct continuation did not publish its first endpoint");
  }
  await presentDirectFrame(renderer);
  const firstEndpointLuma = await sampleRenderedNeighborhood(canvas, 2, -1);
  const waitDirective = JSON.parse(renderer.directWakeDirectiveJson(2000));
  const noSyntheticWaitDraw = renderer.render() === false;
  if (waitDirective.cadence !== "timer" || waitDirective.delayMs < 999 || waitDirective.delayMs > 1001) {
    throw new Error(`direct continuation did not retain its shared wait deadline: ${JSON.stringify(waitDirective)}`);
  }

  if (!renderer.advanceDirectRealtime(3000)) {
    throw new Error("direct continuation did not resume from its wait endpoint");
  }
  await presentDirectFrame(renderer);
  const resumedEditLuma = await sampleRenderedNeighborhood(canvas, 3, -1);

  if (!renderer.advanceDirectRealtime(3500)) {
    throw new Error("direct continuation did not publish its second midpoint");
  }
  await presentDirectFrame(renderer);
  const secondMidpointLuma = await sampleRenderedNeighborhood(canvas, 4, -1);

  if (!renderer.advanceDirectRealtime(4000)) {
    throw new Error("direct continuation did not publish its final endpoint");
  }
  await presentDirectFrame(renderer);
  const finalLuma = await sampleRenderedNeighborhood(canvas, 5, -1);
  const finalDirective = JSON.parse(renderer.directWakeDirectiveJson(4000));
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    firstMidpointLuma,
    firstEndpointLuma,
    resumedEditLuma,
    secondMidpointLuma,
    finalLuma,
    noSyntheticWaitDraw,
    waitDelayMs: waitDirective.delayMs,
    finalCadence: finalDirective.cadence,
  };
  if (metrics.backend !== expectedBackend || metrics.authoredTime !== 4) {
    throw new Error(`direct continuation selected an invalid runtime ${JSON.stringify(metrics)}`);
  }
  if (metrics.objectCount !== 1 || metrics.drawCalls <= 0) {
    throw new Error(`direct continuation produced invalid renderer metrics ${JSON.stringify(metrics)}`);
  }
  if (
    firstMidpointLuma < 180 ||
    firstEndpointLuma < 180 ||
    resumedEditLuma < 180 ||
    secondMidpointLuma < 180 ||
    finalLuma < 180 ||
    !noSyntheticWaitDraw ||
    metrics.finalCadence !== "idle"
  ) {
    throw new Error(`direct continuation pixels or lifecycle are invalid ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function directOrdinaryFadePlayProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectOrdinaryFadePlaySmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow || initial.cadence !== "animation-frame") {
    throw new Error(`direct ordinary fade did not start FadeIn: ${JSON.stringify(initial)}`);
  }
  await presentDirectFrame(renderer);
  const detachedLuma = await sampleRenderedNeighborhood(canvas, 0, 0);

  if (!renderer.advanceDirectRealtime(500)) {
    throw new Error("direct ordinary fade did not publish the FadeIn midpoint");
  }
  await presentDirectFrame(renderer);
  const fadeInMidpointLuma = await sampleRenderedNeighborhood(canvas, 0, 0);

  if (!renderer.advanceDirectRealtime(1000)) {
    throw new Error("direct ordinary fade did not publish the FadeIn endpoint");
  }
  await presentDirectFrame(renderer);
  const fadeInEndpointLuma = await sampleRenderedNeighborhood(canvas, 0, 0);
  const fadeOutStart = JSON.parse(renderer.directWakeDirectiveJson(1000));
  if (fadeOutStart.cadence !== "animation-frame") {
    throw new Error(`direct ordinary fade did not begin FadeOut: ${JSON.stringify(fadeOutStart)}`);
  }

  if (!renderer.advanceDirectRealtime(1500)) {
    throw new Error("direct ordinary fade did not publish the FadeOut midpoint");
  }
  await presentDirectFrame(renderer);
  const fadeOutMidpointLuma = await sampleRenderedNeighborhood(canvas, 0, 0);

  if (!renderer.advanceDirectRealtime(2000)) {
    throw new Error("direct ordinary fade did not publish its absent endpoint");
  }
  await presentDirectFrame(renderer);
  const absentLuma = await sampleRenderedNeighborhood(canvas, 0, 0);
  const absentObjectCount = renderer.objectCount();
  const waitDirective = JSON.parse(renderer.directWakeDirectiveJson(2000));
  const noSyntheticDetachedDraw = renderer.render() === false;
  if (waitDirective.cadence !== "timer" || waitDirective.delayMs < 249 || waitDirective.delayMs > 251) {
    throw new Error(`direct ordinary fade did not retain its detached wait: ${JSON.stringify(waitDirective)}`);
  }

  if (!renderer.advanceDirectRealtime(2250)) {
    throw new Error("direct ordinary fade did not publish same-handle re-entry");
  }
  await presentDirectFrame(renderer);
  const readdedColor = await sampleRenderedColor(canvas, 0, 0);
  const readdedObjectCount = renderer.objectCount();
  const finalBarrier = JSON.parse(renderer.directWakeDirectiveJson(2250));
  if (finalBarrier.cadence !== "timer" || finalBarrier.delayMs !== 0) {
    throw new Error(`direct ordinary fade did not expose its final admission barrier: ${JSON.stringify(finalBarrier)}`);
  }
  if (renderer.advanceDirectRealtime(2250)) {
    throw new Error("direct ordinary fade created a synthetic frame after final admission");
  }
  const finalDirective = JSON.parse(renderer.directWakeDirectiveJson(2250));

  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    detachedLuma,
    fadeInMidpointLuma,
    fadeInEndpointLuma,
    fadeOutMidpointLuma,
    absentLuma,
    absentObjectCount,
    readdedColor,
    readdedObjectCount,
    noSyntheticDetachedDraw,
    waitDelayMs: waitDirective.delayMs,
    finalCadence: finalDirective.cadence,
  };
  if (metrics.backend !== expectedBackend || metrics.authoredTime !== 2.25) {
    throw new Error(`direct ordinary fade selected an invalid runtime ${JSON.stringify(metrics)}`);
  }
  if (
    detachedLuma > 30 ||
    fadeInMidpointLuma < 100 ||
    fadeInMidpointLuma >= fadeInEndpointLuma ||
    fadeInEndpointLuma < 300 ||
    fadeOutMidpointLuma < 100 ||
    fadeOutMidpointLuma >= fadeInEndpointLuma ||
    absentLuma > 30 ||
    absentObjectCount !== 0 ||
    readdedObjectCount !== 1 ||
    readdedColor.red > 40 ||
    readdedColor.green < 80 ||
    readdedColor.blue < 200 ||
    !noSyntheticDetachedDraw ||
    finalDirective.cadence !== "idle"
  ) {
    throw new Error(`direct ordinary fade pixels or lifecycle are invalid ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function directOrdinaryAffineCallbackContinuationProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectOrdinaryAffineCallbackContinuationSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow || initial.cadence !== "animation-frame") {
    throw new Error(
      `direct callback continuation did not start its shared animation: ${JSON.stringify(initial)}`,
    );
  }
  await presentDirectFrame(renderer);
  const initialColor = await sampleRenderedColor(canvas, 0, 1);
  const initialVacatedLuma = await sampleRenderedNeighborhood(canvas, 0, 0);

  await advanceDirectCallbackFrame(renderer, 500);
  const midpointColor = await sampleRenderedColor(canvas, 1, 1);
  const midpointVacatedLuma = await sampleRenderedNeighborhood(canvas, 1, 0);

  await advanceDirectCallbackFrame(renderer, 1000);
  const endpointColor = await sampleRenderedColor(canvas, 2, 1);
  const endpointVacatedLuma = await sampleRenderedNeighborhood(canvas, 2, 0);
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    initialColor,
    initialVacatedLuma,
    midpointColor,
    endpointColor,
    finalCadence: JSON.parse(renderer.directWakeDirectiveJson(1000)).cadence,
    midpointVacatedLuma,
    endpointVacatedLuma,
  };
  if (metrics.backend !== expectedBackend || metrics.authoredTime !== 1 || metrics.finalCadence !== "idle") {
    throw new Error(`direct callback continuation selected an invalid runtime ${JSON.stringify(metrics)}`);
  }
  if (metrics.objectCount !== 1 || metrics.drawCalls <= 0) {
    throw new Error(`direct callback continuation produced invalid renderer metrics ${JSON.stringify(metrics)}`);
  }
  if (
    initialColor.blue < 70 ||
    initialColor.blue > 180 ||
    initialVacatedLuma > 60 ||
    midpointColor.blue < 70 ||
    endpointColor.blue < 70 ||
    Math.abs(midpointColor.blue - initialColor.blue) > 5 ||
    Math.abs(endpointColor.blue - initialColor.blue) > 5 ||
    midpointVacatedLuma > 60 ||
    endpointVacatedLuma > 60
  ) {
    throw new Error(`direct callback continuation pixels or lifecycle are invalid ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function directLineMatchProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectLineMatchSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);
  renderer.directWakeDirectiveJson(0);
  await presentDirectFrame(renderer);
  const middle = await sampleRenderedColor(canvas, 1.25, 0);
  if (renderer.rendererBackend() !== expectedBackend || middle.red < 100 ||
      middle.green > 120 || middle.blue > 120) {
    throw new Error(`ordered Line callback did not preserve its red paint: ${JSON.stringify(middle)}`);
  }
  return { middle };
}

async function directCallbackPaintProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectCallbackPaintSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);
  renderer.directWakeDirectiveJson(0);
  await presentDirectFrame(renderer);
  await advanceDirectCallbackFrame(renderer, 500);
  const midpoint = await sampleRenderedColor(canvas, 1, 0);
  await advanceDirectCallbackFrame(renderer, 1000);
  const endpoint = await sampleRenderedColor(canvas, 2, 0);
  for (const color of [midpoint, endpoint]) {
    if (Math.abs(color.red - 41) > 12 || Math.abs(color.green - 20) > 12 || color.blue > 25) {
      throw new Error(`callback paint did not preserve fill/composite opacity: ${JSON.stringify(color)}`);
    }
  }
  if (renderer.rendererBackend() !== expectedBackend || renderer.time() !== 1) {
    throw new Error("callback paint used an unexpected backend or authored time");
  }
  return { midpoint, endpoint };
}

async function directOrdinaryCallbackSparseReadsProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectOrdinaryCallbackSparseReadsSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow || initial.cadence !== "animation-frame") {
    throw new Error(`direct sparse reads did not start: ${JSON.stringify(initial)}`);
  }
  await settleDirectPublication(renderer, 0);
  const initialRead = await sampleRenderedColor(canvas, -1, 1);
  const initialVacatedLuma = await sampleRenderedNeighborhood(canvas, 0, 0);

  renderer.advanceDirectRealtime(250);
  let trackStart = JSON.parse(renderer.directWakeDirectiveJson(250));
  if (trackStart.presentNow) trackStart = await settleDirectPublication(renderer, 250);
  if (trackStart.cadence !== "animation-frame") {
    throw new Error(`direct sparse reads did not begin its scalar track: ${JSON.stringify(trackStart)}`);
  }
  if (!renderer.advanceDirectRealtime(750)) {
    throw new Error("direct sparse reads did not publish its scalar midpoint");
  }
  await settleDirectPublication(renderer, 750);
  const midpoint = await sampleRenderedColor(canvas, 0, 1);

  if (!renderer.advanceDirectRealtime(1250)) {
    throw new Error("direct sparse reads did not publish its scalar endpoint");
  }
  await settleDirectPublication(renderer, 1250);
  if (!renderer.advanceDirectRealtime(1500)) {
    throw new Error("direct sparse reads did not publish its persistent Hold");
  }
  await settleDirectPublication(renderer, 1500);
  const persistentHold = await sampleRenderedColor(canvas, 2, 1);
  const anchor = await sampleRenderedColor(canvas, -1, 1);
  const finalDirective = JSON.parse(renderer.directWakeDirectiveJson(1500));
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    initialRead,
    initialVacatedLuma,
    midpoint,
    persistentHold,
    anchor,
    finalCadence: finalDirective.cadence,
  };
  if (
    metrics.backend !== expectedBackend ||
    metrics.authoredTime !== 1.5 ||
    metrics.objectCount !== 2 ||
    metrics.drawCalls <= 0 ||
    metrics.finalCadence !== "idle" ||
    metrics.initialVacatedLuma > 60
  ) {
    throw new Error(`direct sparse-read lifecycle is invalid ${JSON.stringify(metrics)}`);
  }
  for (const [label, color] of Object.entries({ initialRead, midpoint, persistentHold, anchor })) {
    if (color.blue < 180 || color.green < 60) {
      throw new Error(`direct sparse-read ${label} is not visibly blue: ${JSON.stringify(metrics)}`);
    }
  }
  return metrics;
}

async function directOrdinaryCompositionPlayProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectOrdinaryCompositionPlaySmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow) {
    throw new Error("direct ordinary composition did not expose its settled publication");
  }
  await presentDirectFrame(renderer);

  const leftColor = await sampleRenderedColor(canvas, -2, 1);
  const rightColor = await sampleRenderedColor(canvas, 2, -1);
  const oldLeftLuma = await sampleRenderedNeighborhood(canvas, -2, 0);
  const oldRightLuma = await sampleRenderedNeighborhood(canvas, 2, 0);
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    leftColor,
    rightColor,
    oldLeftLuma,
    oldRightLuma,
  };
  if (metrics.backend !== expectedBackend) {
    throw new Error(
      `direct ordinary composition selected ${metrics.backend}; expected ${expectedBackend}`,
    );
  }
  if (metrics.authoredTime !== 4 || metrics.objectCount !== 2 || metrics.drawCalls <= 0) {
    throw new Error(`direct ordinary composition produced invalid metrics ${JSON.stringify(metrics)}`);
  }
  if (
    leftColor.green < 180 ||
    leftColor.green < leftColor.red + 100 ||
    leftColor.green < leftColor.blue + 100 ||
    rightColor.blue < 180 ||
    rightColor.blue < rightColor.red + 100 ||
    rightColor.blue < rightColor.green + 100 ||
    oldLeftLuma > 60 ||
    oldRightLuma > 60
  ) {
    throw new Error(`direct ordinary composition pixels are invalid ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function directOrdinaryCompositionContinuationProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectOrdinaryCompositionContinuationSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow || initial.cadence !== "animation-frame") {
    throw new Error(`direct composition continuation did not start: ${JSON.stringify(initial)}`);
  }
  await settleDirectPublication(renderer, 0);
  if (!renderer.advanceDirectRealtime(1000)) {
    throw new Error("direct composition continuation did not publish its parallel midpoint");
  }
  await settleDirectPublication(renderer, 1000);
  const leftMidpoint = await sampleRenderedColor(canvas, -2, 0.5);
  const rightMidpoint = await sampleRenderedColor(canvas, 2, -0.5);

  if (!renderer.advanceDirectRealtime(2000)) {
    throw new Error("direct composition continuation did not complete its parallel segment");
  }
  await settleDirectPublication(renderer, 2000);
  if (!renderer.advanceDirectRealtime(3000)) {
    throw new Error("direct composition continuation did not publish its sequence midpoint");
  }
  await settleDirectPublication(renderer, 3000);
  const leftSequence = await sampleRenderedColor(canvas, -2, 1);
  const rightSequence = await sampleRenderedColor(canvas, 2, -1);

  if (!renderer.advanceDirectRealtime(4000)) {
    throw new Error("direct composition continuation did not complete its sequence");
  }
  await settleDirectPublication(renderer, 4000);
  const leftFinal = await sampleRenderedColor(canvas, -2, 1);
  const rightFinal = await sampleRenderedColor(canvas, 2, -1);
  const finalDirective = JSON.parse(renderer.directWakeDirectiveJson(4000));
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    leftMidpoint,
    rightMidpoint,
    leftSequence,
    rightSequence,
    leftFinal,
    rightFinal,
    finalCadence: finalDirective.cadence,
  };
  if (metrics.backend !== expectedBackend || metrics.authoredTime !== 4) {
    throw new Error(`direct composition continuation selected invalid runtime ${JSON.stringify(metrics)}`);
  }
  if (metrics.objectCount !== 2 || metrics.drawCalls <= 0 || metrics.finalCadence !== "idle") {
    throw new Error(`direct composition continuation lifecycle is invalid ${JSON.stringify(metrics)}`);
  }
  if (
    leftMidpoint.red < 120 || leftMidpoint.green < 120 || leftMidpoint.blue < 120 ||
    rightMidpoint.red < 120 || rightMidpoint.green < 120 || rightMidpoint.blue < 120 ||
    leftSequence.red < leftSequence.green + 40 || leftSequence.red < leftSequence.blue + 40 ||
    rightSequence.red < 120 || rightSequence.green < 120 || rightSequence.blue < 120 ||
    leftFinal.green < leftFinal.red + 40 || leftFinal.green < leftFinal.blue + 40 ||
    rightFinal.blue < rightFinal.red + 40 || rightFinal.blue < rightFinal.green + 40
  ) {
    throw new Error(`direct composition continuation colors are invalid ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function directOrdinaryValueTrackerContinuationProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectOrdinaryValueTrackerContinuationSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow || initial.cadence !== "animation-frame") {
    throw new Error(`direct scalar continuation did not start: ${JSON.stringify(initial)}`);
  }
  await settleDirectPublication(renderer, 0);

  if (!renderer.advanceDirectRealtime(1000)) {
    throw new Error("direct scalar continuation did not publish its first midpoint");
  }
  await settleDirectPublication(renderer, 1000);
  const firstMidpoint = await sampleRenderedColor(canvas, -1, 0);

  if (!renderer.advanceDirectRealtime(2000)) {
    throw new Error("direct scalar continuation did not complete its first track");
  }
  await settleDirectPublication(renderer, 2000);
  if (renderer.advanceDirectRealtime(2500) || renderer.render()) {
    throw new Error("direct scalar static hold produced unnecessary execution or draw work");
  }
  const persistentHold = await sampleRenderedColor(canvas, 1, 0);
  renderer.advanceDirectRealtime(3000);
  const afterWait = JSON.parse(renderer.directWakeDirectiveJson(3000));
  if (afterWait.presentNow) await settleDirectPublication(renderer, 3000);
  if (afterWait.cadence !== "animation-frame") {
    throw new Error("direct scalar wait did not resume its second track");
  }

  if (!renderer.advanceDirectRealtime(3500)) {
    throw new Error("direct scalar continuation did not publish its second midpoint");
  }
  await settleDirectPublication(renderer, 3500);
  const secondMidpoint = await sampleRenderedColor(canvas, 2, 0);

  if (!renderer.advanceDirectRealtime(4000)) {
    throw new Error("direct scalar continuation did not complete its second track");
  }
  await settleDirectPublication(renderer, 4000);
  const endpoint = await sampleRenderedColor(canvas, 3, 0);
  const finalDirective = JSON.parse(renderer.directWakeDirectiveJson(4000));
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    firstMidpoint,
    persistentHold,
    secondMidpoint,
    endpoint,
    finalCadence: finalDirective.cadence,
  };
  if (metrics.backend !== expectedBackend || metrics.authoredTime !== 4) {
    throw new Error(`direct scalar continuation selected invalid runtime ${JSON.stringify(metrics)}`);
  }
  if (metrics.objectCount !== 1 || metrics.drawCalls <= 0 || metrics.finalCadence !== "idle") {
    throw new Error(`direct scalar continuation lifecycle is invalid ${JSON.stringify(metrics)}`);
  }
  for (const [label, color] of Object.entries({
    firstMidpoint, persistentHold, secondMidpoint, endpoint,
  })) {
    if (color.red < 180 || color.green < 180 || color.blue < 180) {
      throw new Error(`direct scalar continuation ${label} is not visibly white: ${JSON.stringify(metrics)}`);
    }
  }
  return metrics;
}

async function directOrdinaryStylePlayProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectOrdinaryStylePlaySmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow) {
    throw new Error("direct ordinary style session did not expose its settled publication");
  }
  await presentDirectFrame(renderer);

  // The shared Rust builder verifies the interpolated and completed style. The
  // final green pixel proves the post-completion authored edit reached this same
  // runtime and renderer.
  const endpointColor = await sampleRenderedColor(canvas, 0, 0);
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    endpointColor,
  };
  if (metrics.backend !== expectedBackend) {
    throw new Error(
      `direct ordinary style renderer selected ${metrics.backend}; expected ${expectedBackend}`,
    );
  }
  if (metrics.authoredTime !== 2) {
    throw new Error(`direct ordinary style authored time is ${metrics.authoredTime}; expected 2`);
  }
  if (metrics.objectCount !== 1 || metrics.drawCalls <= 0) {
    throw new Error(`direct ordinary style produced invalid metrics ${JSON.stringify(metrics)}`);
  }
  if (
    endpointColor.green < 180 ||
    endpointColor.green < endpointColor.red + 100 ||
    endpointColor.green < endpointColor.blue + 100
  ) {
    throw new Error(`direct ordinary style did not render its green authored edit ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function directOrdinaryPaintPlayProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectOrdinaryPaintPlaySmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const initial = JSON.parse(renderer.directWakeDirectiveJson(0));
  if (!initial.presentNow) {
    throw new Error("direct ordinary paint session did not expose its settled publication");
  }
  await presentDirectFrame(renderer);

  // The shared Rust builder verifies both paint channels at their midpoint and
  // endpoint. This final yellow pixel proves that completion released them for
  // a later ordinary set_color/set_opacity publication.
  const endpointColor = await sampleRenderedColor(canvas, 0, 0);
  const metrics = {
    backend: renderer.rendererBackend(),
    authoredTime: renderer.time(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    endpointColor,
  };
  if (metrics.backend !== expectedBackend) {
    throw new Error(
      `direct ordinary paint renderer selected ${metrics.backend}; expected ${expectedBackend}`,
    );
  }
  if (metrics.authoredTime !== 2.4) {
    throw new Error(`direct ordinary paint authored time is ${metrics.authoredTime}; expected 2.4`);
  }
  if (metrics.objectCount !== 1 || metrics.drawCalls <= 0) {
    throw new Error(`direct ordinary paint produced invalid metrics ${JSON.stringify(metrics)}`);
  }
  if (
    endpointColor.red < 180 ||
    endpointColor.green < 180 ||
    endpointColor.blue > 100
  ) {
    throw new Error(`direct ordinary paint did not render its yellow authored edit ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function directNativeSignalsProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectNativeSignalsSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  await presentDirectFrame(renderer);
  const hiddenLuma = await sampleRenderedPixel(canvas, 0, 0);
  if (renderer.objectCount() !== 0 || hiddenLuma > 60) {
    throw new Error(
      `direct native signals did not apply initial key presence: ${JSON.stringify({
        objectCount: renderer.objectCount(),
        hiddenLuma,
      })}`,
    );
  }

  if (!renderer.setSpaceKey(true)) {
    throw new Error("direct native key state did not publish its presence change");
  }
  await presentDirectFrame(renderer);
  const visibleLuma = await sampleRenderedNeighborhood(canvas, 0, 0, 2);

  if (!renderer.setPointerPosition(1.5, -0.5)) {
    throw new Error("direct native pointer state did not publish its translation change");
  }
  await presentDirectFrame(renderer);
  const vacatedLuma = await sampleRenderedNeighborhood(canvas, 0, 0, 2);
  const movedLuma = await sampleRenderedNeighborhood(canvas, 1.5, -0.5, 2);

  if (!renderer.setOpacityControl(0.4)) {
    throw new Error("direct native control state did not publish its opacity change");
  }
  await presentDirectFrame(renderer);
  const dimmedLuma = await sampleRenderedNeighborhood(canvas, 1.5, -0.5, 2);

  if (!renderer.emitPrimaryPointerDown()) {
    throw new Error("direct native pointer event did not publish its first ordered occurrence");
  }
  await presentDirectFrame(renderer);
  const firstClickLuma = await sampleRenderedNeighborhood(canvas, 0.99, -0.61, 2);

  if (!renderer.emitPrimaryPointerDown()) {
    throw new Error("direct native pointer event did not publish its second ordered occurrence");
  }
  await presentDirectFrame(renderer);
  const secondClickLuma = await sampleRenderedNeighborhood(canvas, 0.99, -0.61, 2);

  const metrics = {
    backend: renderer.rendererBackend(),
    objectCount: renderer.objectCount(),
    drawCalls: renderer.lastDrawCalls(),
    hiddenLuma,
    visibleLuma,
    vacatedLuma,
    movedLuma,
    dimmedLuma,
    firstClickLuma,
    secondClickLuma,
  };
  if (metrics.backend !== expectedBackend) {
    throw new Error(
      `direct native signals renderer selected ${metrics.backend}; expected ${expectedBackend}`,
    );
  }
  if (metrics.objectCount !== 1 || metrics.drawCalls <= 0) {
    throw new Error(`direct native signals produced invalid metrics ${JSON.stringify(metrics)}`);
  }
  if (visibleLuma < 200 || vacatedLuma > 60 || movedLuma < 200) {
    throw new Error(`direct native pointer/presence pixels are invalid ${JSON.stringify(metrics)}`);
  }
  if (dimmedLuma < 60 || dimmedLuma >= movedLuma * 0.7) {
    throw new Error(`direct native opacity did not dim the moved object ${JSON.stringify(metrics)}`);
  }
  if (firstClickLuma < 60 || secondClickLuma > 60) {
    throw new Error(`direct native ordered clicks did not rotate the object ${JSON.stringify(metrics)}`);
  }
  return metrics;
}

async function start() {
  if (typeof createDirectExecutionSmokeRenderer !== "function") {
    state.metrics = {
      skipped: true,
      reason: "debug-only direct execution proof is unavailable in this production package",
    };
    state.ready = true;
    return;
  }
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
  metrics.callbackPaint = await directCallbackPaintProof(expectedBackend);
  metrics.lineMatch = await directLineMatchProof(expectedBackend);
  metrics.affineCompletion = await directAffineCompletionProof(expectedBackend);
  metrics.ordinaryAffinePlay = await directOrdinaryAffinePlayProof(expectedBackend);
  metrics.ordinaryAffineContinuation = await directOrdinaryAffineContinuationProof(expectedBackend);
  metrics.ordinaryFadePlay = await directOrdinaryFadePlayProof(expectedBackend);
  metrics.ordinaryAffineCallbackContinuation =
    await directOrdinaryAffineCallbackContinuationProof(expectedBackend);
  metrics.ordinaryCallbackSparseReads =
    await directOrdinaryCallbackSparseReadsProof(expectedBackend);
  metrics.ordinaryCompositionPlay = await directOrdinaryCompositionPlayProof(expectedBackend);
  metrics.ordinaryCompositionContinuation =
    await directOrdinaryCompositionContinuationProof(expectedBackend);
  metrics.ordinaryValueTrackerContinuation =
    await directOrdinaryValueTrackerContinuationProof(expectedBackend);
  metrics.ordinaryStylePlay = await directOrdinaryStylePlayProof(expectedBackend);
  metrics.ordinaryPaintPlay = await directOrdinaryPaintPlayProof(expectedBackend);
  metrics.nativeSignals = await directNativeSignalsProof(expectedBackend);

  state.metrics = metrics;
  state.ready = true;
}

start().catch((error) => {
  state.error = String(error);
  state.ready = true;
  console.error(error);
});
