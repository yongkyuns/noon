import init, { NoonCanvasPlayer } from "./pkg/noon_web.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { BrowserJankMonitor } from "./browser-jank.js";
import { FrameMetrics, SampleWindow } from "./frame-metrics.js";

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

let client = null;
let player = null;
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
  player = await NoonCanvasPlayer.create(canvas, sceneJson, 4.0);
  const playerCreateMs = performance.now() - createStarted;
  player.resize(canvas.width, canvas.height);
  player.setCamera(0, 0, cameraHeight);
  const gpuSupported = player.gpuProfilingSupported();
  player.setGpuProfilingEnabled(gpuSupported);

  for (let frame = 0; frame < warmupFrames; frame += 1) {
    status.value = `Warm-up ${frame + 1}/${warmupFrames} · ${sourcePath}…`;
    player.renderFrame(await nextAnimationFrame());
  }

  player.resetClock();
  player.resetGpuProfiling();
  const cadence = new FrameMetrics({ targetHz });
  const windows = {
    cpu: new SampleWindow(measuredFrames),
    runtime: new SampleWindow(measuredFrames),
    prepare: new SampleWindow(measuredFrames),
    upload: new SampleWindow(measuredFrames),
    encode: new SampleWindow(measuredFrames),
  };
  const jank = new BrowserJankMonitor();
  const measurementStart = performance.now();
  jank.start();
  let measured = 0;
  while (measured < measuredFrames) {
    status.value = `Measuring ${measured + 1}/${measuredFrames} · ${sourcePath}…`;
    const timestamp = await nextAnimationFrame();
    const submitStarted = performance.now();
    const presented = player.renderFrame(timestamp);
    const browserSubmitMs = performance.now() - submitStarted;
    if (!presented) continue;
    cadence.record(timestamp, browserSubmitMs);
    windows.cpu.record(player.lastCpuFrameMs());
    windows.runtime.record(player.lastRuntimeEvaluationMs());
    windows.prepare.record(player.lastFramePrepareMs());
    windows.upload.record(player.lastUploadMs());
    windows.encode.record(player.lastEncodeSubmitMs());
    measured += 1;
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
      objects: player.objectCount(),
      cameraHeight,
    },
    environment: {
      userAgent: navigator.userAgent,
      rendererBackend: player.rendererBackend(),
      devicePixelRatio: window.devicePixelRatio || 1,
      backingResolution: [canvas.width, canvas.height],
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
      frameIntervalMs: frame.interval,
      effective: frame.cadence,
      browserSubmitMs: frame.submission,
    },
    cpu: {
      frameMs: windows.cpu.summary(),
      runtimeMs: windows.runtime.summary(),
      prepareMs: windows.prepare.summary(),
      uploadMs: windows.upload.summary(),
      encodeSubmitMs: windows.encode.summary(),
    },
    renderer: {
      drawCalls: player.lastDrawCalls(),
      instances: player.lastInstancesDrawn(),
      lastUploadBytes: player.lastBytesUploaded(),
      geometryCacheMisses: player.lastGeometryCacheMisses(),
    },
    browser: {
      longTasks: jank.summary(measurementStart, measurementEnd),
    },
    gpu: gpuSupported
      ? {
          samples: player.gpuProfiledFrameCount(),
          dropped: player.gpuDroppedSampleCount(),
          failed: player.gpuFailedSampleCount(),
          p50: finite(player.gpuRenderP50Ms()),
          p95: finite(player.gpuRenderP95Ms()),
          p99:
            typeof player.gpuRenderP99Ms === "function"
              ? finite(player.gpuRenderP99Ms())
              : null,
        }
      : { supported: false },
  };

  window.__NOON_SCENE_PERF__ = report;
  output.textContent = JSON.stringify(report, null, 2);
  status.value =
    `Complete · ${sourcePath} · ${format(report.cadence.effective?.effectiveFps)} FPS · ` +
    `p95 ${format(report.cadence.frameIntervalMs?.p95)} ms`;
  status.dataset.state = "complete";
  console.log("NOON_SCENE_PERF", report);
} catch (error) {
  console.error(error);
  status.value = `Scene profile failed: ${error}`;
  status.dataset.state = "error";
} finally {
  client?.terminate();
  player?.free?.();
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

function finite(value) {
  return Number.isFinite(value) ? value : null;
}

function format(value) {
  return Number.isFinite(value) ? Number(value).toFixed(2) : "—";
}
