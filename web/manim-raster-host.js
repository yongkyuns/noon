import initNoonWeb, {
  EngineScenePlayer,
  ExecutionCanvasRenderer,
} from "./pkg/noon_web.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { AuthoringExecutionClient } from "./authoring-execution-client.js";

const canvas = document.querySelector("#scene");
const client = new PythonAuthoringClient();
const readyPromise = Promise.all([initNoonWeb(), client.ready()]);

let engine = null;
let execution = null;
let sourceFailure = null;
let renderer = null;
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

async function load(source, loopDurationSeconds, { mode = "semantic" } = {}) {
  await readyPromise;
  if (typeof source !== "string" || source.trim() === "") {
    throw new TypeError("host raster source must be non-empty");
  }
  const loopDuration = Number(loopDurationSeconds);
  if (!Number.isFinite(loopDuration) || loopDuration <= 0) {
    throw new RangeError("host raster loop duration must be positive and finite");
  }
  if (renderer !== null || engine !== null || execution !== null) {
    throw new Error("host raster page supports one authored scene per page");
  }

  if (mode === "semantic") {
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
    });
    await attached;
    await execution.sampleToAuthoredTime(0);
    const metrics = (await execution.metrics()).metrics;
    return {
      kind: "semantic_execution",
      duration: authoredDuration,
      objectCount: metrics.objectCount,
      rendererBackend: metrics.rendererBackend,
    };
  }
  if (mode !== "document") throw new Error(`unsupported raster mode ${mode}`);

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
    throw new Error("document raster fixtures cannot execute host callbacks");
  }
  authoredDuration = Number(result.duration);

  // The initial execution delta already carries either the scene's semantic camera
  // state or the shared default camera. Do not overwrite it with a harness-local
  // fixed camera; moving-camera fixtures must exercise the production camera role.
  const initialDelta = engine.initialDeltaJson();
  renderer = await ExecutionCanvasRenderer.create(canvas.transferControlToOffscreen(), initialDelta);
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
  };
}

async function advanceOneFrame(frameIndex, time) {
  if (sourceFailure !== null) throw sourceFailure;
  if (execution !== null) {
    const sampled = await execution.sampleToAuthoredTime(time);
    currentLogicalTime = sampled.time;
  } else {
    await presentDelta(engine.tickDeltaJson(time * 1000));
    currentLogicalTime = time;
  }
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
  if (execution === null && (renderer === null || engine === null)) {
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

  if (execution !== null) {
    const metrics = (await execution.metrics()).metrics;
    return {
      error: null,
      presented: true,
      time: currentLogicalTime,
      objectCount: metrics.objectCount,
      rendererBackend: metrics.rendererBackend,
      drawCalls: metrics.drawCalls,
      authoredDuration,
      frameIndex: currentFrameIndex,
    };
  }

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
