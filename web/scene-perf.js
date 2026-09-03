import init, { EngineScenePlayer, ExecutionCanvasRenderer } from "./pkg/noon_web.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { BrowserJankMonitor } from "./browser-jank.js";
import { FrameMetrics, SampleWindow } from "./frame-metrics.js";
import {
  drainRendererGpuDiagnostics,
  formatGpuDiagnostic,
} from "./render-gpu-diagnostics.js";

const LOOP_DURATION_SECONDS = 4;
const parameters = new URLSearchParams(location.search);
const sourcePath = parameters.get("source") ?? "./python/demo_scene.py";
if (!sourcePath.startsWith("./python/") || !sourcePath.endsWith(".py")) {
  throw new Error("scene performance source must be a local ./python/*.py file");
}
const warmupFrames = positiveInteger("warmup", 30);
const measuredFrames = positiveInteger("frames", 180);
const targetHz = positiveNumber("targetHz", 60);
const cameraHeight = positiveNumber("cameraHeight", 6);
const context = parseContext(parameters.get("context"));
const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");
const output = document.querySelector("#json");
const backingWidth = canvas.width;
const backingHeight = canvas.height;

let client = null;
let engine = null;
let renderer = null;
try {
  await init();
  const source = await loadText(sourcePath);
  const workerStarted = performance.now();
  client = new PythonAuthoringClient();
  await client.ready();
  const workerStartupMs = performance.now() - workerStarted;

  status.value = `Authoring ${sourcePath}…`;
  const authorStarted = performance.now();
  const authored = await client.run(source, context);
  const authoringMs = performance.now() - authorStarted;
  if (authored.kind !== "scene_document") {
    throw new Error("performance corpus source did not return a Scene");
  }

  const jsonStarted = performance.now();
  const sceneJson = JSON.stringify(authored.document);
  const serializationMs = performance.now() - jsonStarted;
  const createStarted = performance.now();
  engine = new EngineScenePlayer(sceneJson, LOOP_DURATION_SECONDS, 1);
  const offscreen = canvas.transferControlToOffscreen();
  renderer = await ExecutionCanvasRenderer.create(offscreen, engine.initialDeltaJson());
  renderer.resize(backingWidth, backingHeight);
  renderer.setCamera(0, 0, cameraHeight);
  if (!presentPending()) {
    throw new Error("initial corpus frame was not presented");
  }
  const playerCreateMs = performance.now() - createStarted;

  for (let frame = 0; frame < warmupFrames; frame += 1) {
    status.value = `Warm-up ${frame + 1}/${warmupFrames} · ${sourcePath}…`;
    advanceFrame(await nextAnimationFrame());
  }

  resetPlayback();
  const cadence = new FrameMetrics({ targetHz });
  const windows = {
    cpu: new SampleWindow(measuredFrames),
    runtime: new SampleWindow(measuredFrames),
    transportApply: new SampleWindow(measuredFrames),
    rendererRender: new SampleWindow(measuredFrames),
  };
  const jank = new BrowserJankMonitor();
  const measurementStart = performance.now();
  jank.start();
  let dirtyFrames = 0;
  let cleanFrames = 0;
  for (let measured = 0; measured < measuredFrames; measured += 1) {
    status.value = `Measuring ${measured + 1}/${measuredFrames} · ${sourcePath}…`;
    const timestamp = await nextAnimationFrame();
    const frameTiming = advanceFrame(timestamp);
    cadence.record(timestamp, frameTiming.cpuFrameMs);
    windows.cpu.record(frameTiming.cpuFrameMs);
    windows.runtime.record(frameTiming.runtimeMs);
    windows.transportApply.record(frameTiming.transportApplyMs);
    windows.rendererRender.record(frameTiming.rendererRenderMs);
    if (frameTiming.dirty) {
      dirtyFrames += 1;
    } else {
      cleanFrames += 1;
    }
  }
  const measurementEnd = performance.now();
  jank.stop();
  const frame = cadence.summary();

  const report = {
    schemaVersion: 1,
    benchmark: "Noon realistic authored scene profile",
    generatedAt: new Date().toISOString(),
    scene: {
      source: sourcePath,
      context,
      objects: renderer.objectCount(),
      cameraHeight,
    },
    environment: {
      userAgent: navigator.userAgent,
      rendererBackend: renderer.rendererBackend(),
      devicePixelRatio: window.devicePixelRatio || 1,
      backingResolution: [backingWidth, backingHeight],
      targetHz,
    },
    setup: {
      workerStartupMs,
      authoringMs,
      serializationMs,
      serializedBytes: new TextEncoder().encode(sceneJson).byteLength,
      playerCreateMs,
      warmupFrames,
    },
    cadence: {
      frames: frame.frames,
      dirtyFrames,
      cleanFrames,
      frameIntervalMs: frame.interval,
      effective: frame.cadence,
      browserSubmitMs: frame.submission,
    },
    cpu: {
      frameMs: windows.cpu.summary(),
      runtimeMs: windows.runtime.summary(),
      transportApplyMs: windows.transportApply.summary(),
      rendererRenderMs: windows.rendererRender.summary(),
      // The split execution renderer exposes aggregate render-host time rather
      // than the deleted monolith's internal prepare/upload/encode phase timers.
      prepareMs: null,
      uploadMs: null,
      encodeSubmitMs: null,
    },
    renderer: {
      drawCalls: renderer.lastDrawCalls(),
      instances: renderer.lastInstancesDrawn(),
      lastUploadBytes: renderer.lastBytesUploaded(),
      geometryCacheMisses: renderer.lastGeometryCacheMisses(),
    },
    browser: {
      longTasks: jank.summary(measurementStart, measurementEnd),
    },
    gpu: {
      supported: false,
      p50: null,
      p95: null,
      p99: null,
      unavailableReason:
        "ExecutionCanvasRenderer does not yet expose WebGPU timestamp-query profiling",
    },
  };

  window.__NOON_SCENE_PERF__ = report;
  output.textContent = JSON.stringify(report, null, 2);
  status.value =
    `Complete · ${sourcePath} · ${format(report.cadence.effective?.effectiveFps)} FPS · ` +
    `p95 ${format(report.cadence.frameIntervalMs?.p95)} ms · ` +
    `${dirtyFrames}/${measuredFrames} dirty frames`;
  status.dataset.state = "complete";
  console.log("NOON_SCENE_PERF", report);
} catch (error) {
  console.error(error);
  status.value = `Scene profile failed: ${error}`;
  status.dataset.state = "error";
} finally {
  client?.terminate();
  renderer?.free?.();
  engine?.free?.();
}

function advanceFrame(timestamp) {
  const frameStarted = performance.now();
  const runtimeStarted = performance.now();
  const delta = engine.tickDeltaJson(timestamp);
  const runtimeMs = performance.now() - runtimeStarted;
  if (delta === undefined || delta === null) {
    return {
      dirty: false,
      cpuFrameMs: performance.now() - frameStarted,
      runtimeMs,
      transportApplyMs: 0,
      rendererRenderMs: 0,
    };
  }

  const applyStarted = performance.now();
  if (!renderer.applyDeltaJson(delta)) {
    throw new Error("renderer rejected a non-stale corpus execution delta");
  }
  // Corpus fixtures do not author a camera object. Keep the benchmark viewport
  // explicit after transport applies the engine's default camera state.
  renderer.setCamera(0, 0, cameraHeight);
  drainGpuDiagnostics();
  const transportApplyMs = performance.now() - applyStarted;

  const renderStarted = performance.now();
  if (!presentPending()) {
    throw new Error("dirty corpus frame was not presented");
  }
  const rendererRenderMs = performance.now() - renderStarted;
  return {
    dirty: true,
    cpuFrameMs: performance.now() - frameStarted,
    runtimeMs,
    transportApplyMs,
    rendererRenderMs,
  };
}

function resetPlayback() {
  const delta = engine.seekDeltaJson(0);
  if (delta === undefined || delta === null) {
    return;
  }
  if (!renderer.applyDeltaJson(delta)) {
    throw new Error("renderer rejected corpus playback reset");
  }
  renderer.setCamera(0, 0, cameraHeight);
  drainGpuDiagnostics();
  if (!presentPending()) {
    throw new Error("corpus playback reset was not presented");
  }
}

function presentPending() {
  for (let attempt = 0; attempt < 4; attempt += 1) {
    drainGpuDiagnostics();
    const presented = renderer.render();
    drainGpuDiagnostics();
    if (presented) {
      return true;
    }
  }
  return false;
}

function drainGpuDiagnostics() {
  let fatal = null;
  const healthy = drainRendererGpuDiagnostics(renderer, {
    onRecoverable(diagnostic) {
      console.warn(formatGpuDiagnostic(diagnostic));
    },
    onFatal(diagnostic) {
      fatal = new Error(formatGpuDiagnostic(diagnostic));
    },
  });
  if (!healthy) {
    throw fatal ?? new Error("renderer reported a fatal GPU diagnostic");
  }
}

function parseContext(value) {
  if (value === null || value === "") return {};
  const parsed = JSON.parse(value);
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    throw new Error("context must decode to an object");
  }
  return parsed;
}

async function loadText(path) {
  const response = await fetch(path);
  if (!response.ok) throw new Error(`Unable to load ${path}: HTTP ${response.status}`);
  return response.text();
}

function nextAnimationFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function positiveInteger(name, fallback) {
  const value = parameters.get(name);
  if (value === null) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`);
  return parsed;
}

function positiveNumber(name, fallback) {
  const value = parameters.get(name);
  if (value === null) return fallback;
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) throw new Error(`${name} must be positive`);
  return parsed;
}

function format(value) {
  return Number.isFinite(value) ? Number(value).toFixed(2) : "—";
}
