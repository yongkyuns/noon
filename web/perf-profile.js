import init, { EngineScenePlayer, ExecutionCanvasRenderer } from "./pkg/noon_web.js";
import { BrowserJankMonitor, estimateUnattributedFrameMs } from "./browser-jank.js";
import { FrameMetrics, SampleWindow } from "./frame-metrics.js";
import { ANALYTIC_LAYOUTS, buildAnalyticScene } from "./perf-workloads.js";
import {
  drainRendererGpuDiagnostics,
  formatGpuDiagnostic,
} from "./render-gpu-diagnostics.js";

const parameters = new URLSearchParams(location.search);
const objectCount = positiveInteger("objects", 10_000);
const warmupFrames = positiveInteger("warmup", 30);
const measuredFrames = positiveInteger("frames", 300);
const targetHz = positiveNumber("targetHz", 60);
const width = positiveInteger("width", 960);
const height = positiveInteger("height", 540);
const layout = parameters.get("layout") ?? "fit";
if (!ANALYTIC_LAYOUTS.includes(layout)) {
  throw new Error(`layout must be one of ${ANALYTIC_LAYOUTS.join(", ")}`);
}

const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");
const output = document.querySelector("#json");
canvas.width = width;
canvas.height = height;
canvas.style.aspectRatio = `${width} / ${height}`;

const driverDurationSeconds = Math.max(
  60,
  ((warmupFrames + measuredFrames + 16) / targetHz) * 2,
);
const browserJank = new BrowserJankMonitor();
let engine = null;
let renderer = null;
let workload = null;

try {
  await init();
  workload = buildAnalyticScene({ count: objectCount, layout, aspect: width / height });
  installIncrementalDriver(workload.document, driverDurationSeconds);

  const sceneJson = JSON.stringify(workload.document);
  const createStarted = performance.now();
  engine = new EngineScenePlayer(sceneJson, driverDurationSeconds, 1);
  const offscreen = canvas.transferControlToOffscreen();
  renderer = await ExecutionCanvasRenderer.create(offscreen, engine.initialDeltaJson());
  renderer.resize(width, height);
  renderer.setCamera(0, 0, workload.cameraHeight);
  if (!presentPending()) {
    throw new Error("initial performance frame was not presented");
  }
  const playerCreateMs = performance.now() - createStarted;

  status.value = `Warming ${layout} / ${objectCount.toLocaleString()} objects…`;
  for (let frame = 0; frame < warmupFrames; frame += 1) {
    await nextAnimationFrame();
    presentSceneTime((frame + 1) / targetHz);
  }

  // Keep the measurement phase deterministic across devices. Browser rAF owns
  // cadence sampling only; semantic scene time advances by one target-Hz step.
  presentSceneTime(0);
  const cadence = new FrameMetrics({ targetHz });
  const windows = {
    cpuFrameMs: new SampleWindow(measuredFrames),
    runtimeMs: new SampleWindow(measuredFrames),
    transportApplyMs: new SampleWindow(measuredFrames),
    rendererRenderMs: new SampleWindow(measuredFrames),
    unattributedFrameMs: new SampleWindow(measuredFrames),
  };
  browserJank.start();
  const measurementStartMs = performance.now();
  let previousTimestamp = null;
  let measured = 0;
  while (measured < measuredFrames) {
    status.value = `Measuring ${measured + 1}/${measuredFrames} · ${layout} / ${objectCount.toLocaleString()} objects…`;
    const timestamp = await nextAnimationFrame();
    const timings = presentSceneTime((measured + 1) / targetHz);

    cadence.record(timestamp, timings.cpuFrameMs);
    windows.cpuFrameMs.record(timings.cpuFrameMs);
    windows.runtimeMs.record(timings.runtimeMs);
    windows.transportApplyMs.record(timings.transportApplyMs);
    windows.rendererRenderMs.record(timings.rendererRenderMs);
    if (previousTimestamp !== null) {
      windows.unattributedFrameMs.record(
        estimateUnattributedFrameMs(timestamp - previousTimestamp, timings.cpuFrameMs),
      );
    }
    previousTimestamp = timestamp;
    measured += 1;
  }
  const measurementEndMs = performance.now();
  browserJank.stop();

  const frame = cadence.summary();
  const report = {
    schemaVersion: 1,
    benchmark: "Noon incremental analytic frame profile",
    generatedAt: new Date().toISOString(),
    workload: {
      family: "analytic-incremental",
      layout,
      description:
        `${workload.description}; one object carries a deterministic position track ` +
        "so each frame crosses the execution-delta boundary without rebuilding the scene",
      objects: objectCount,
      incrementalDriverObjects: 1,
      driverDurationSeconds,
    },
    environment: {
      userAgent: navigator.userAgent,
      rendererBackend: renderer.rendererBackend(),
      hardwareConcurrency: navigator.hardwareConcurrency ?? null,
      deviceMemoryGiB: navigator.deviceMemory ?? null,
      devicePixelRatio: window.devicePixelRatio || 1,
      canvasCssSize: [canvas.clientWidth, canvas.clientHeight],
      backingResolution: [width, height],
      targetHz,
      observedRefreshHz: frame.interval?.p50 > 0 ? 1000 / frame.interval.p50 : null,
      crossOriginIsolated: window.crossOriginIsolated,
    },
    setup: {
      playerCreateMs,
      warmupFrames,
      measuredFrames: frame.frames,
      incompleteMetricFrames: 0,
    },
    cadence: {
      frameIntervalMs: frame.interval,
      effective: frame.cadence,
      browserRenderCallMs: frame.submission,
      unattributedFrameMs: windows.unattributedFrameMs.summary(),
    },
    browser: {
      longTasks: browserJank.summary(measurementStartMs, measurementEndMs),
    },
    cpu: {
      frameMs: windows.cpuFrameMs.summary(),
      runtimeEvaluationMs: windows.runtimeMs.summary(),
      transportApplyMs: windows.transportApplyMs.summary(),
      rendererRenderMs: windows.rendererRenderMs.summary(),
      // The split execution renderer currently exposes aggregate render-host
      // timing rather than the deleted monolith's prepare/upload/encode timers.
      framePrepareMs: null,
      uploadMs: null,
      encodeSubmitMs: null,
    },
    renderer: {
      drawCalls: renderer.lastDrawCalls(),
      instances: renderer.lastInstancesDrawn(),
      lastUploadBytes: renderer.lastBytesUploaded(),
      geometryCacheMisses: renderer.lastGeometryCacheMisses(),
    },
    gpu: {
      timestampSupported: false,
      unavailableReason:
        "ExecutionCanvasRenderer does not yet expose WebGPU timestamp-query profiling",
    },
  };

  window.__NOON_PERF_REPORT__ = report;
  output.textContent = JSON.stringify(report, null, 2);
  status.value =
    `Complete · ${formatNumber(report.cadence.effective?.effectiveFps)} FPS · ` +
    `frame p95 ${formatNumber(report.cadence.frameIntervalMs?.p95)} ms · ` +
    `${report.cadence.effective?.longFrames ?? 0} long frames`;
  status.dataset.state = "complete";
  console.log("NOON_PERF_REPORT", report);
} catch (error) {
  console.error(error);
  status.value = `Profile failed: ${error}`;
  status.dataset.state = "error";
} finally {
  browserJank.stop();
  renderer?.free?.();
  engine?.free?.();
}

function installIncrementalDriver(document, durationSeconds) {
  if (!Array.isArray(document.objects) || document.objects.length === 0) {
    throw new Error("performance workload must contain at least one object");
  }
  if (!Array.isArray(document.tracks) || document.tracks.length !== 0) {
    throw new Error("analytic performance workload must start without animation tracks");
  }

  const object = document.objects[0];
  const from = object.transform?.translation;
  if (!from || !Number.isFinite(from.x) || !Number.isFinite(from.y)) {
    throw new Error("performance driver object must have a finite translation");
  }
  document.tracks.push({
    id: 0,
    object: object.id,
    property: "position",
    values: {
      vec2: {
        from: { x: from.x, y: from.y },
        to: { x: from.x + 8, y: from.y },
      },
    },
    timing: {
      start_time: 0,
      duration: durationSeconds,
      easing: "linear",
    },
  });
}

function presentSceneTime(sceneTime) {
  const frameStarted = performance.now();

  const runtimeStarted = performance.now();
  const delta = engine.seekDeltaJson(sceneTime);
  const runtimeMs = performance.now() - runtimeStarted;
  if (delta === undefined || delta === null) {
    throw new Error(`incremental performance driver emitted no delta at t=${sceneTime}`);
  }

  const applyStarted = performance.now();
  if (!renderer.applyDeltaJson(delta)) {
    throw new Error(`renderer rejected incremental performance delta at t=${sceneTime}`);
  }
  // The synthetic SceneDefinition has no authored camera object. Keep the
  // profiling viewport explicit after each transport delta updates mirror state.
  renderer.setCamera(0, 0, workload.cameraHeight);
  drainGpuDiagnostics();
  const transportApplyMs = performance.now() - applyStarted;

  const renderStarted = performance.now();
  if (!presentPending()) {
    throw new Error(`performance frame was not presented at t=${sceneTime}`);
  }
  const rendererRenderMs = performance.now() - renderStarted;

  return {
    cpuFrameMs: performance.now() - frameStarted,
    runtimeMs,
    transportApplyMs,
    rendererRenderMs,
  };
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

function nextAnimationFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function positiveInteger(name, fallback) {
  const value = parameters.get(name);
  if (value === null) {
    return fallback;
  }
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return parsed;
}

function positiveNumber(name, fallback) {
  const value = parameters.get(name);
  if (value === null) {
    return fallback;
  }
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive number`);
  }
  return parsed;
}

function formatNumber(value) {
  return Number.isFinite(value) ? Number(value).toFixed(2) : "—";
}
