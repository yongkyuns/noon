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

let engine = null;
let host = null;
let renderer = null;
let callbackSessionId = null;
let currentFrameIndex = -1;
let currentLogicalTime = 0;
let activeFrameTimes = null;
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

  // This #959-owned codec/renderer diagnostic explicitly consumes a scene
  // document. Canonical callback execution is qualified by the shared-authoring
  // and direct Rust/WASM proofs without this legacy document adapter.
  const result = await client.run(source, {}, { exportDocument: true });
  if (result.kind !== "scene_document") {
    throw new Error("host raster harness requires a scene document");
  }

  const sceneJson = JSON.stringify(result.document);
  engine = new EngineScenePlayer(sceneJson, loopDuration, 1);
  if (result.callbacks !== null) {
    host = new HostScenePlayer(sceneJson, JSON.stringify(result.callbacks.slots));
    callbackSessionId = result.callbacks.session_id;
  }
  authoredDuration = Number(result.duration);

  // The initial execution delta already carries either the scene's semantic camera
  // state or the shared default camera. Do not overwrite it with a harness-local
  // fixed camera; moving-camera fixtures must exercise the production camera role.
  const initialDelta = engine.initialDeltaJson();
  renderer = await ExecutionCanvasRenderer.create(offscreen, initialDelta);
  renderer.resize(canvas.width, canvas.height);
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
    callbackSlots: result.callbacks?.slots.length ?? 0,
  };
}

async function advanceOneFrame(frameIndex, time) {
  const deterministicDelta = engine.tickDeltaJson(time * 1000);
  await presentDelta(deterministicDelta);

  if (host !== null) {
    host.advanceTo(time);
    const frame = JSON.parse(host.callbackFrameJson());
    if (Math.abs(Number(frame.time) - Number(time)) > 1e-9) {
      throw new Error(
        `host callback playhead mismatch at frame ${frameIndex}: expected ${time}, got ${frame.time}`,
      );
    }
    const sequence = Number(host.nextSequence());
    const batch = await client.runCallbackPhase(callbackSessionId, frame, sequence);
    const batchJson = JSON.stringify(batch);
    host.commitPatchBatch(batchJson);

    const hostDelta = engine.applyHostPatchBatchDeltaJson(batchJson);
    await presentDelta(hostDelta);
  }
  currentFrameIndex = frameIndex;
  currentLogicalTime = time;
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
  if (renderer === null || engine === null) {
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

  return {
    error: null,
    presented: true,
    // Logical scene time advances every reference frame even when the execution
    // transport correctly emits no visual delta (for example a zero-dt updater
    // activation boundary). Keep the renderer's last-delta time separately for
    // diagnostics instead of treating it as the authoritative playhead.
    time: currentLogicalTime,
    rendererTime: renderer.time(),
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
