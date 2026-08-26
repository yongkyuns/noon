import initNoonWeb, {
  EngineScenePlayer,
  ExecutionCanvasRenderer,
  HostScenePlayer,
} from "./pkg/noon_web.js";
import { PythonAuthoringClient } from "./authoring-client.js";

const canvas = document.querySelector("#scene");
const offscreen = canvas.transferControlToOffscreen();
const client = new PythonAuthoringClient();
const readyPromise = Promise.all([initNoonWeb(), client.ready()]);
const MANIM_DEFAULT_CAMERA_HEIGHT = 8.0;

let engine = null;
let host = null;
let renderer = null;
let callbackSessionId = null;
let currentFrameIndex = -1;
let activeFrameRate = null;
let authoredDuration = null;

function waitForPaint() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(resolve));
  });
}

async function presentDelta(deltaJson) {
  if (deltaJson === undefined || deltaJson === null) return false;
  const applied = renderer.applyDeltaJson(deltaJson);
  if (!applied) return false;
  let presented = false;
  for (let attempt = 0; attempt < 4 && !presented; attempt += 1) {
    presented = renderer.render();
  }
  if (!presented) {
    throw new Error("host raster renderer could not present an applied execution delta");
  }
  await waitForPaint();
  return true;
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
  if (renderer !== null || engine !== null || host !== null) {
    throw new Error("host raster page supports one authored scene per page");
  }

  const result = await client.run(source);
  if (result.kind !== "scene_document") {
    throw new Error("host raster harness requires a scene document");
  }
  if (result.callbacks === null) {
    throw new Error("host raster harness requires registered host callbacks");
  }

  const sceneJson = JSON.stringify(result.document);
  engine = new EngineScenePlayer(sceneJson, loopDuration, 1);
  host = new HostScenePlayer(sceneJson, JSON.stringify(result.callbacks.slots));
  callbackSessionId = result.callbacks.session_id;
  authoredDuration = Number(result.duration);

  const initialDelta = engine.initialDeltaJson();
  renderer = await ExecutionCanvasRenderer.create(offscreen, initialDelta);
  renderer.resize(canvas.width, canvas.height);
  renderer.setCamera(0.0, 0.0, MANIM_DEFAULT_CAMERA_HEIGHT);
  let presented = false;
  for (let attempt = 0; attempt < 4 && !presented; attempt += 1) {
    presented = renderer.render();
  }
  if (!presented) {
    throw new Error("host raster renderer could not present its initial snapshot");
  }
  await waitForPaint();

  return {
    kind: result.kind,
    duration: authoredDuration,
    objectCount: result.document.objects.length,
    rendererBackend: renderer.rendererBackend(),
    callbackSlots: result.callbacks.slots.length,
  };
}

async function advanceOneFrame(frameIndex, frameRate) {
  const time = frameIndex / frameRate;

  const deterministicDelta = engine.tickDeltaJson(time * 1000);
  await presentDelta(deterministicDelta);

  host.advanceTo(time);
  const frame = JSON.parse(host.callbackFrameJson());
  const sequence = Number(host.nextSequence());
  const batch = await client.runCallbackPhase(callbackSessionId, frame, sequence);
  const batchJson = JSON.stringify(batch);
  host.commitPatchBatch(batchJson);

  const hostDelta = engine.applyHostPatchBatchDeltaJson(batchJson);
  await presentDelta(hostDelta);
  currentFrameIndex = frameIndex;
}

async function renderThrough(frameIndex, frameRate) {
  if (renderer === null || engine === null || host === null) {
    throw new Error("host raster scene has not been loaded");
  }
  const targetFrame = Number(frameIndex);
  const fps = Number(frameRate);
  if (!Number.isSafeInteger(targetFrame) || targetFrame < 0) {
    throw new RangeError("host raster frame index must be a non-negative integer");
  }
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new RangeError("host raster frame rate must be positive and finite");
  }
  if (activeFrameRate === null) activeFrameRate = fps;
  if (activeFrameRate !== fps) {
    throw new Error("host raster frame rate cannot change after playback begins");
  }
  if (targetFrame < currentFrameIndex) {
    throw new RangeError("host raster playback cannot move backwards");
  }

  for (let frame = currentFrameIndex + 1; frame <= targetFrame; frame += 1) {
    await advanceOneFrame(frame, fps);
  }
  await waitForPaint();

  return {
    error: null,
    presented: true,
    time: renderer.time(),
    objectCount: renderer.objectCount(),
    rendererBackend: renderer.rendererBackend(),
    drawCalls: renderer.lastDrawCalls(),
    instances: renderer.lastInstancesDrawn(),
    uploadBytes: renderer.lastBytesUploaded(),
    geometryCacheMisses: renderer.lastGeometryCacheMisses(),
    authoredDuration,
    frameIndex: currentFrameIndex,
  };
}

window.noonHostRaster = {
  ready: () => readyPromise,
  load,
  renderThrough,
};
