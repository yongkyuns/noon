import { PythonAuthoringClient } from "./authoring-client.js";
import { AuthoringExecutionClient } from "./authoring-execution-client.js";
import { SemanticPreviewSession } from "./semantic-preview-session.js";

const canvas = document.querySelector("#scene");
const readyPromise = Promise.resolve();

let preview = null;
let closed = false;
let currentFrameIndex = -1;
let currentLogicalTime = 0;
let activeFrameTimes = null;

function waitForPaint() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(resolve));
  });
}

async function load(source, loopDurationSeconds) {
  await readyPromise;
  if (closed) throw new Error("host raster page is closed");
  if (typeof source !== "string" || source.trim() === "") {
    throw new TypeError("host raster source must be non-empty");
  }
  const loopDuration = Number(loopDurationSeconds);
  if (!Number.isFinite(loopDuration) || loopDuration <= 0) {
    throw new RangeError("host raster loop duration must be positive and finite");
  }
  if (preview !== null) {
    throw new Error("host raster page supports one authored scene per page");
  }

  preview = new SemanticPreviewSession({
    createAuthoringClient: () => new PythonAuthoringClient(),
    createExecutionClient: (options) => new AuthoringExecutionClient(canvas, options),
  });
  const result = await preview.open(source, { loopDurationSeconds: loopDuration });
  return {
    kind: "semantic_execution",
    duration: result.authoredDuration,
    objectCount: result.frame.objectCount,
    rendererBackend: result.frame.rendererBackend,
  };
}

async function advanceOneFrame(frameIndex, time) {
  if (closed) throw new Error("host raster page is closed");
  const sampled = await preview.sample(time);
  currentLogicalTime = sampled.frame.publishedTime;
  currentFrameIndex = frameIndex;
}

function normalizeFrameTimes(frameTimes, targetFrame) {
  if (!Array.isArray(frameTimes) || frameTimes.length <= targetFrame) {
    throw new RangeError("host raster frame-time map must cover the target frame");
  }
  return frameTimes.map((value, index) => {
    const time = Number(value);
    if (!Number.isFinite(time) || time < 0) {
      throw new RangeError(`host raster frame ${index} has invalid logical time ${value}`);
    }
    if (index > 0 && time + 1e-12 < Number(frameTimes[index - 1])) {
      throw new RangeError("host raster frame-time map must be monotonic");
    }
    return time;
  });
}

async function renderThrough(frameIndex, frameTimes) {
  if (closed) throw new Error("host raster page is closed");
  if (preview === null) throw new Error("host raster scene has not been loaded");
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
  if (closed) throw new Error("host raster page is closed");

  const report = preview.snapshot;
  if (report.state !== "ready") {
    throw new Error(report.error ?? "preview session is not ready");
  }
  return {
    error: null,
    presented: true,
    time: currentLogicalTime,
    objectCount: report.frame.objectCount,
    rendererBackend: report.frame.rendererBackend,
    drawCalls: report.frame.drawCalls,
    authoredDuration: report.authoredDuration,
    frameIndex: currentFrameIndex,
  };
}

function close() {
  if (closed) return;
  closed = true;
  preview?.close();
}

window.noonHostRaster = {
  ready: () => readyPromise,
  load,
  renderThrough,
  status: () => preview?.snapshot ?? null,
  close,
};
