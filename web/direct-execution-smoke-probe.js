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

function nextAnimationFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
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

async function driveFromSessionWakePlan(renderer) {
  let presented = false;
  let animationFrames = 0;
  let timerWakes = 0;
  let wallOriginMilliseconds = null;
  let sceneOriginSeconds = renderer.time();

  for (let wake = 0; wake < 240; wake += 1) {
    if (renderer.directPresentPending()) {
      if (!renderer.render()) {
        await sleep(0);
        continue;
      }
      presented = true;
    }

    const cadence = renderer.directWakeCadence();
    if (cadence === "idle") {
      if (renderer.directPresentPending()) {
        continue;
      }
      return {
        presented,
        animationFrames,
        timerWakes,
        wakeCycles: wake + 1,
        settled: true,
      };
    }

    if (cadence === "animation-frame") {
      const wallNowMilliseconds = await nextAnimationFrame();
      if (wallOriginMilliseconds === null) {
        wallOriginMilliseconds = wallNowMilliseconds;
        sceneOriginSeconds = renderer.time();
      }
      const sceneTime =
        sceneOriginSeconds + (wallNowMilliseconds - wallOriginMilliseconds) / 1000;
      renderer.advanceDirectToSceneTime(sceneTime);
      animationFrames += 1;
      continue;
    }

    if (cadence === "timer") {
      const delaySeconds = renderer.directTimerDelaySeconds();
      if (!Number.isFinite(delaySeconds)) {
        throw new Error("direct execution timer cadence did not expose a finite delay");
      }
      await sleep(Math.max(0, delaySeconds * 1000));
      renderer.advanceDirectToSceneTime(renderer.time() + delaySeconds);
      wallOriginMilliseconds = null;
      timerWakes += 1;
      continue;
    }

    throw new Error(`direct execution renderer exposed unknown wake cadence: ${cadence}`);
  }

  throw new Error("direct execution renderer did not settle within bounded wake cycles");
}

async function start() {
  const expectedBackend = await waitForPrimaryRenderer();
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectExecutionSmokeRenderer(canvas);
  renderer.resize(canvas.width, canvas.height);

  const wakeMetrics = await driveFromSessionWakePlan(renderer);
  const metrics = {
    backend: renderer.rendererBackend(),
    ...wakeMetrics,
    objectCount: renderer.objectCount(),
    finalSceneTime: renderer.time(),
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
  if (!metrics.settled) {
    throw new Error("direct execution renderer did not settle after active authored work");
  }
  if (metrics.animationFrames <= 0) {
    throw new Error("direct execution renderer never requested animation-frame cadence");
  }
  if (metrics.finalSceneTime <= 0) {
    throw new Error(`direct execution renderer never advanced authored time: ${metrics.finalSceneTime}`);
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
