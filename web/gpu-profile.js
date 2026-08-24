import init, { NoonCanvasPlayer } from "./pkg/noon_web.js";
import { FrameMetrics } from "./frame-metrics.js";
import { ANALYTIC_LAYOUTS, buildAnalyticScene } from "./perf-workloads.js";

const parameters = new URLSearchParams(location.search);
const objectCount = positiveInteger("objects", 10_000);
const warmupFrames = positiveInteger("warmup", 30);
const measuredFrames = positiveInteger("frames", 180);
const targetHz = positiveNumber("targetHz", 60);
const layout = parameters.get("layout") ?? "fit";
if (!ANALYTIC_LAYOUTS.includes(layout)) {
  throw new Error(`layout must be one of ${ANALYTIC_LAYOUTS.join(", ")}`);
}
const canvas = document.querySelector("#scene");
const result = document.querySelector("#result");

try {
  if (!navigator.gpu) {
    throw new Error("This browser does not expose WebGPU");
  }
  await init();
  const workload = buildAnalyticScene({
    count: objectCount,
    layout,
    aspect: canvas.width / canvas.height,
  });
  const player = await NoonCanvasPlayer.create(canvas, JSON.stringify(workload.document), 1);
  player.resize(canvas.width, canvas.height);
  player.setCamera(0, 0, workload.cameraHeight);
  const gpuSupported = player.gpuProfilingSupported();
  player.setGpuProfilingEnabled(true);

  const metrics = new FrameMetrics({ targetHz });
  let renderedFrames = 0;
  let measured = 0;
  result.value = `Warming ${layout} / ${objectCount.toLocaleString()} objects…`;
  result.dataset.state = "warming";
  requestAnimationFrame(render);

  function render(timestamp) {
    const started = performance.now();
    const presented = player.renderFrame(timestamp);
    const submissionMs = performance.now() - started;
    if (presented) {
      renderedFrames += 1;
      if (renderedFrames === warmupFrames) {
        player.resetGpuProfiling();
        metrics.reset();
      } else if (renderedFrames > warmupFrames) {
        metrics.record(timestamp, submissionMs);
        measured += 1;
      }
    }

    if (measured < measuredFrames) {
      requestAnimationFrame(render);
    } else {
      waitForGpuSamples(performance.now() + 2_000);
    }
  }

  function waitForGpuSamples(deadline) {
    const accounted =
      player.gpuProfiledFrameCount() +
      player.gpuDroppedSampleCount() +
      player.gpuFailedSampleCount();
    if (!gpuSupported || accounted >= measuredFrames || performance.now() >= deadline) {
      finish();
    } else {
      requestAnimationFrame(() => waitForGpuSamples(deadline));
    }
  }

  function finish() {
    const frame = metrics.summary();
    const profile = {
      objects: objectCount,
      layout,
      workload: workload.description,
      viewport: [canvas.width, canvas.height],
      targetHz,
      measuredFrames: frame.frames,
      drawCalls: player.lastDrawCalls(),
      instances: player.lastInstancesDrawn(),
      lastUploadBytes: player.lastBytesUploaded(),
      submissionMs: frame.submission,
      frameIntervalMs: frame.interval,
      cadence: frame.cadence,
      gpuTimestampSupported: gpuSupported,
      gpuRenderPassMs: gpuSupported
        ? {
            samples: player.gpuProfiledFrameCount(),
            dropped: player.gpuDroppedSampleCount(),
            failed: player.gpuFailedSampleCount(),
            p50: finiteOrNull(player.gpuRenderP50Ms()),
            p95: finiteOrNull(player.gpuRenderP95Ms()),
            p99:
              typeof player.gpuRenderP99Ms === "function"
                ? finiteOrNull(player.gpuRenderP99Ms())
                : null,
            last: finiteOrNull(player.lastGpuRenderMs()),
          }
        : null,
    };
    result.value = JSON.stringify(profile, null, 2);
    result.dataset.state = "complete";
    result.dataset.objects = String(profile.objects);
    result.dataset.layout = profile.layout;
    result.dataset.submitP95Ms = String(profile.submissionMs?.p95 ?? "");
    result.dataset.intervalP95Ms = String(profile.frameIntervalMs?.p95 ?? "");
    result.dataset.gpuSupported = String(profile.gpuTimestampSupported);
    result.dataset.gpuP95Ms = String(profile.gpuRenderPassMs?.p95 ?? "");
    console.log("NOON_GPU_PROFILE", profile);
  }
} catch (error) {
  console.error(error);
  result.value = `Profile failed: ${error}`;
  result.dataset.state = "error";
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
