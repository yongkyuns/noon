import init, { createDirectAnalyticProfileRenderer } from "./pkg/noon_web.js";
import { BrowserJankMonitor, estimateUnattributedFrameMs } from "./browser-jank.js";
import { FrameMetrics, SampleWindow } from "./frame-metrics.js";
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
const ANALYTIC_LAYOUTS = ["fit", "fixed", "overdraw"];
if (objectCount > 100_000) throw new Error("objects must not exceed 100000");
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
let renderer = null;

try {
  await init();
  const createStarted = performance.now();
  const offscreen = canvas.transferControlToOffscreen();
  renderer = await createDirectAnalyticProfileRenderer(
    offscreen, objectCount, layout, width / height, driverDurationSeconds,
  );
  renderer.resize(width, height);
  renderer.advanceDirectRealtime(0);
  renderer.enableGpuTimestampProfiling(true);
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
  renderer.seekDirect(0);
  renderer.advanceDirectRealtime(0);
  presentPending();
  const warmupGpuTimestamps = await settleGpuTimestampMetrics(
    new SampleWindow(warmupFrames + 8),
  );
  if (
    warmupGpuTimestamps.timestampSupported &&
    !renderer.resetGpuTimestampMetrics()
  ) {
    throw new Error("GPU timestamp warmup readbacks did not drain before measurement");
  }
  const cadence = new FrameMetrics({ targetHz });
  const windows = {
    cpuFrameMs: new SampleWindow(measuredFrames),
    runtimeMs: new SampleWindow(measuredFrames),
    rendererRenderMs: new SampleWindow(measuredFrames),
    unattributedFrameMs: new SampleWindow(measuredFrames),
    gpuRenderPassMs: new SampleWindow(measuredFrames),
  };
  browserJank.start();
  const measurementStartMs = performance.now();
  let previousTimestamp = null;
  let measured = 0;
  while (measured < measuredFrames) {
    status.value = `Measuring ${measured + 1}/${measuredFrames} · ${layout} / ${objectCount.toLocaleString()} objects…`;
    const timestamp = await nextAnimationFrame();
    const timings = presentSceneTime((measured + 1) / targetHz);
    takeGpuTimestampMetrics(windows.gpuRenderPassMs);

    cadence.record(timestamp, timings.cpuFrameMs);
    windows.cpuFrameMs.record(timings.cpuFrameMs);
    windows.runtimeMs.record(timings.runtimeMs);
    windows.rendererRenderMs.record(timings.rendererRenderMs);
    if (previousTimestamp !== null) {
      windows.unattributedFrameMs.record(
        estimateUnattributedFrameMs(timestamp - previousTimestamp, timings.cpuFrameMs),
      );
    }
    previousTimestamp = timestamp;
    measured += 1;
  }
  const gpuTimestamps = await settleGpuTimestampMetrics(windows.gpuRenderPassMs);
  const measurementEndMs = performance.now();
  browserJank.stop();

  const frame = cadence.summary();
  const report = {
    schemaVersion: 2,
    benchmark: "Noon direct Rust analytic frame profile",
    generatedAt: new Date().toISOString(),
    workload: {
      family: "analytic-direct",
      layout,
      description: "shared Rust analytic workload; one object advances on a linear position track",
      execution: "typed-direct-rust",
      camera: "authored",
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
      runtimeAdvanceMs: windows.runtimeMs.summary(),
      rendererRenderMs: windows.rendererRenderMs.summary(),
      // Direct advancement and rendering are measured at their typed host calls;
      // renderer-internal prepare/upload/encode timers are not exposed here.
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
      timestampSupported: gpuTimestamps.timestampSupported,
      unavailableReason: gpuTimestamps.timestampSupported
        ? null
        : "WebGPU TIMESTAMP_QUERY is unavailable on this renderer device",
      samples: gpuTimestamps.samples,
      dropped: gpuTimestamps.dropped,
      failed: gpuTimestamps.failed,
      inFlight: gpuTimestamps.inFlight,
      renderPassMs: windows.gpuRenderPassMs.summary(),
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
}

function presentSceneTime(sceneTime) {
  const frameStarted = performance.now();

  const runtimeStarted = performance.now();
  const pending = renderer.advanceDirectRealtime(sceneTime * 1000);
  const runtimeMs = performance.now() - runtimeStarted;
  if (Math.abs(renderer.time() - sceneTime) > 1e-6) {
    throw new Error(`direct runtime did not reach requested sample ${sceneTime}`);
  }
  if (!pending) {
    throw new Error(`analytic driver produced no pending direct frame at t=${sceneTime}`);
  }
  drainGpuDiagnostics();

  const renderStarted = performance.now();
  if (!presentPending()) {
    throw new Error(`performance frame was not presented at t=${sceneTime}`);
  }
  const rendererRenderMs = performance.now() - renderStarted;

  return {
    cpuFrameMs: performance.now() - frameStarted,
    runtimeMs,
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

function takeGpuTimestampMetrics(sampleWindow) {
  const raw = renderer.takeGpuTimestampJson();
  const metrics = JSON.parse(raw);
  if (!metrics || typeof metrics !== "object") {
    throw new Error("renderer returned invalid GPU timestamp diagnostics");
  }
  const renderPassMs = Array.isArray(metrics.renderPassMs) ? metrics.renderPassMs : [];
  for (const milliseconds of renderPassMs) {
    sampleWindow.record(milliseconds);
  }
  return {
    timestampSupported: metrics.timestampSupported === true,
    samples: nonnegativeInteger(metrics.samples),
    dropped: nonnegativeInteger(metrics.dropped),
    failed: nonnegativeInteger(metrics.failed),
    inFlight: nonnegativeInteger(metrics.inFlight),
  };
}

async function settleGpuTimestampMetrics(sampleWindow) {
  let metrics = takeGpuTimestampMetrics(sampleWindow);
  // Mapping resolves after queue submission. Yield browser frames to process
  // those callbacks, bounded so profiling never turns into a synchronous wait.
  for (let attempt = 0; metrics.timestampSupported && metrics.inFlight > 0 && attempt < 32; attempt += 1) {
    await nextAnimationFrame();
    metrics = takeGpuTimestampMetrics(sampleWindow);
  }
  return metrics;
}

function nonnegativeInteger(value) {
  return Number.isSafeInteger(value) && value >= 0 ? value : 0;
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
