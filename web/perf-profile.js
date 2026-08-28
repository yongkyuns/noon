import init, { NoonCanvasPlayer } from "./pkg/noon_web.js";
import { BrowserJankMonitor, estimateUnattributedFrameMs } from "./browser-jank.js";
import { FrameMetrics, SampleWindow } from "./frame-metrics.js";
import { readCompletePlayerFrameMetrics } from "./player-frame-metrics.js";
import { ANALYTIC_LAYOUTS, buildAnalyticScene } from "./perf-workloads.js";

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

let player = null;
const browserJank = new BrowserJankMonitor();
try {
  await init();
  const workload = buildAnalyticScene({ count: objectCount, layout, aspect: width / height });
  const createStarted = performance.now();
  player = await NoonCanvasPlayer.create(canvas, JSON.stringify(workload.document), 4.0);
  const playerCreateMs = performance.now() - createStarted;
  player.resize(width, height);
  player.setCamera(0, 0, workload.cameraHeight);

  const gpuSupported = player.gpuProfilingSupported();
  player.setGpuProfilingEnabled(gpuSupported);
  player.resetGpuProfiling();

  status.value = `Warming ${layout} / ${objectCount.toLocaleString()} objects…`;
  for (let frame = 0; frame < warmupFrames; frame += 1) {
    const timestamp = await nextAnimationFrame();
    player.renderFrame(timestamp);
  }

  player.resetGpuProfiling();
  player.resetClock();
  const cadence = new FrameMetrics({ targetHz });
  const windows = {
    cpuFrameMs: new SampleWindow(measuredFrames),
    runtimeMs: new SampleWindow(measuredFrames),
    prepareMs: new SampleWindow(measuredFrames),
    uploadMs: new SampleWindow(measuredFrames),
    encodeSubmitMs: new SampleWindow(measuredFrames),
    unattributedFrameMs: new SampleWindow(measuredFrames),
  };
  browserJank.start();
  const measurementStartMs = performance.now();
  let previousTimestamp = null;
  let incompleteMetricFrames = 0;
  let measured = 0;
  while (measured < measuredFrames) {
    status.value = `Measuring ${measured + 1}/${measuredFrames} · ${layout} / ${objectCount.toLocaleString()} objects…`;
    const timestamp = await nextAnimationFrame();
    const started = performance.now();
    const presented = player.renderFrame(timestamp);
    const browserCallMs = performance.now() - started;
    if (!presented) {
      previousTimestamp = null;
      continue;
    }

    const metrics = readCompletePlayerFrameMetrics(player);
    if (metrics === null) {
      // GPU/context recovery can replace renderer-owned instrumentation between
      // runtime evaluation and presentation. Exclude that whole frame rather than
      // mixing partial measurements into percentile windows. The skipped count is
      // reported so repeated recovery remains visible instead of being hidden.
      incompleteMetricFrames += 1;
      previousTimestamp = null;
      continue;
    }

    cadence.record(timestamp, browserCallMs);
    windows.cpuFrameMs.record(metrics.cpuFrameMs);
    windows.runtimeMs.record(metrics.runtimeMs);
    windows.prepareMs.record(metrics.prepareMs);
    windows.uploadMs.record(metrics.uploadMs);
    windows.encodeSubmitMs.record(metrics.encodeSubmitMs);
    if (previousTimestamp !== null) {
      windows.unattributedFrameMs.record(
        estimateUnattributedFrameMs(timestamp - previousTimestamp, metrics.cpuFrameMs),
      );
    }
    previousTimestamp = timestamp;
    measured += 1;
  }
  const measurementEndMs = performance.now();
  browserJank.stop();

  await waitForGpuSamples(player, gpuSupported, measuredFrames);
  const frame = cadence.summary();
  const report = {
    schemaVersion: 1,
    benchmark: "Noon end-to-end analytic frame profile",
    generatedAt: new Date().toISOString(),
    workload: {
      family: "analytic-static",
      layout,
      description: workload.description,
      objects: objectCount,
    },
    environment: {
      userAgent: navigator.userAgent,
      rendererBackend: player.rendererBackend(),
      hardwareConcurrency: navigator.hardwareConcurrency ?? null,
      deviceMemoryGiB: navigator.deviceMemory ?? null,
      devicePixelRatio: window.devicePixelRatio || 1,
      canvasCssSize: [canvas.clientWidth, canvas.clientHeight],
      backingResolution: [width, height],
      targetHz,
      observedRefreshHz:
        frame.interval?.p50 > 0 ? 1000 / frame.interval.p50 : null,
      crossOriginIsolated: window.crossOriginIsolated,
    },
    setup: {
      playerCreateMs,
      warmupFrames,
      measuredFrames: frame.frames,
      incompleteMetricFrames,
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
      framePrepareMs: windows.prepareMs.summary(),
      uploadMs: windows.uploadMs.summary(),
      encodeSubmitMs: windows.encodeSubmitMs.summary(),
    },
    renderer: {
      drawCalls: player.lastDrawCalls(),
      instances: player.lastInstancesDrawn(),
      lastUploadBytes: player.lastBytesUploaded(),
      geometryCacheMisses: player.lastGeometryCacheMisses(),
    },
    gpu: gpuSupported
      ? {
          timestampSupported: true,
          samples: player.gpuProfiledFrameCount(),
          dropped: player.gpuDroppedSampleCount(),
          failed: player.gpuFailedSampleCount(),
          renderPassMs: {
            p50: finiteOrNull(player.gpuRenderP50Ms()),
            p95: finiteOrNull(player.gpuRenderP95Ms()),
            p99:
              typeof player.gpuRenderP99Ms === "function"
                ? finiteOrNull(player.gpuRenderP99Ms())
                : null,
            last: finiteOrNull(player.lastGpuRenderMs()),
          },
        }
      : { timestampSupported: false },
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
  player?.free?.();
}

async function waitForGpuSamples(player, supported, expectedFrames) {
  if (!supported) {
    return;
  }
  const deadline = performance.now() + 2_000;
  while (performance.now() < deadline) {
    const accounted =
      player.gpuProfiledFrameCount() +
      player.gpuDroppedSampleCount() +
      player.gpuFailedSampleCount();
    if (accounted >= expectedFrames) {
      return;
    }
    await nextAnimationFrame();
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

function finiteOrNull(value) {
  return Number.isFinite(value) ? value : null;
}

function formatNumber(value) {
  return Number.isFinite(value) ? Number(value).toFixed(2) : "—";
}
