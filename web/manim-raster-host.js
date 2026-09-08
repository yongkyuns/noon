import { PythonAuthoringClient } from "./authoring-client.js";
import { AuthoringExecutionClient } from "./authoring-execution-client.js";

const canvas = document.querySelector("#scene");
const client = new PythonAuthoringClient();
const readyPromise = client.ready();

let execution = null;
let sourceFailure = null;
let currentFrameIndex = -1;
let currentLogicalTime = 0;
let activeFrameTimes = null;
let authoredDuration = null;

function waitForPaint() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(resolve));
  });
}

async function load(source, loopDurationSeconds) {
  await readyPromise;
  if (typeof source !== "string" || source.trim() === "") {
    throw new TypeError("host raster source must be non-empty");
  }
  const loopDuration = Number(loopDurationSeconds);
  if (!Number.isFinite(loopDuration) || loopDuration <= 0) {
    throw new RangeError("host raster loop duration must be positive and finite");
  }
  if (execution !== null) {
    throw new Error("host raster page supports one authored scene per page");
  }

  let resolveAttached;
  let rejectAttached;
  const attached = new Promise((resolve, reject) => {
    resolveAttached = resolve;
    rejectAttached = reject;
  });
  const sourceRun = client.run(source, {}, {
    async onSemanticContinuation(registration) {
      if (execution !== null) throw new Error("raster source registered a second execution context");
      execution = new AuthoringExecutionClient(canvas);
      await execution.startSemanticExecution(registration.semanticExecution, {
        authoringClient: client,
        loopDurationSeconds: loopDuration,
        transportMode: "transferable",
        pacing: "external_samples",
      });
      resolveAttached();
    },
  });
  sourceRun.then((result) => {
    authoredDuration = result.duration;
    if (execution === null) rejectAttached(new Error("raster source produced no continuation"));
  }, (error) => {
    sourceFailure = error;
    rejectAttached(error);
    execution?.terminate();
  });
  await attached;
  await sampleSharedSource(0);
  const metrics = (await execution.metrics()).metrics;
  return {
    kind: "semantic_execution",
    duration: authoredDuration,
    objectCount: metrics.objectCount,
    rendererBackend: metrics.backend,
  };
}

async function sampleSharedSource(time) {
  if (sourceFailure !== null) throw sourceFailure;
  try {
    return await execution.sampleToAuthoredTime(time);
  } catch (error) {
    throw sourceFailure ?? error;
  }
}

async function advanceOneFrame(frameIndex, time) {
  const sampled = await sampleSharedSource(time);
  currentLogicalTime = sampled.time;
  currentFrameIndex = frameIndex;
}

function normalizeFrameTimes(frameTimes, targetFrame) {
  if (!Array.isArray(frameTimes) || frameTimes.length <= targetFrame) {
    throw new RangeError("host raster frame-time map must cover the target frame");
  }
  const normalized = frameTimes.map((value, index) => {
    const time = Number(value);
    if (!Number.isFinite(time) || time < 0) {
      throw new RangeError(`host raster frame ${index} has invalid logical time ${value}`);
    }
    if (index > 0 && time + 1e-12 < Number(frameTimes[index - 1])) {
      throw new RangeError("host raster frame-time map must be monotonic");
    }
    return time;
  });
  return normalized;
}

async function renderThrough(frameIndex, frameTimes) {
  if (execution === null) {
    throw new Error("host raster scene has not been loaded");
  }
  const targetFrame = Number(frameIndex);
  if (!Number.isSafeInteger(targetFrame) || targetFrame < 0) {
    throw new RangeError("host raster frame index must be a non-negative integer");
  }
  const normalizedTimes = normalizeFrameTimes(frameTimes, targetFrame);
  if (activeFrameTimes === null) {
    activeFrameTimes = normalizedTimes;
  } else {
    if (activeFrameTimes.length !== normalizedTimes.length) {
      throw new Error("host raster frame-time map cannot change after playback begins");
    }
    for (let index = 0; index < activeFrameTimes.length; index += 1) {
      if (Math.abs(activeFrameTimes[index] - normalizedTimes[index]) > 1e-12) {
        throw new Error("host raster frame-time map cannot change after playback begins");
      }
    }
  }
  if (targetFrame < currentFrameIndex) {
    throw new RangeError("host raster playback cannot move backwards");
  }

  for (let frame = currentFrameIndex + 1; frame <= targetFrame; frame += 1) {
    await advanceOneFrame(frame, activeFrameTimes[frame]);
  }
  await waitForPaint();

  const metrics = (await execution.metrics()).metrics;
  return {
    error: null,
    presented: true,
    time: currentLogicalTime,
    objectCount: metrics.objectCount,
    rendererBackend: metrics.backend,
    drawCalls: metrics.drawCalls,
    authoredDuration,
    frameIndex: currentFrameIndex,
  };
}

window.noonHostRaster = {
  ready: () => readyPromise,
  load,
  renderThrough,
};
