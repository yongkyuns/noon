import init, { NoonCanvasPlayer } from "./pkg/noon_web.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { SampleWindow } from "./frame-metrics.js";

const CASES = [600, 1000, 3000];
const AUTHORING_IMPORT_WARMUP_OBJECTS = 12;
const AUTHORING_SAMPLES = 5;
const WARMUP_FRAMES = 30;
const MEASURED_FRAMES = 180;
const GPU_SETTLE_TIMEOUT_MS = 2000;

const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");
const setup = document.querySelector("#setup");
const resultsBody = document.querySelector("#results");
const jsonOutput = document.querySelector("#json");

let authoringClient = null;
let activePlayer = null;
let authoringSetup = null;

try {
  if (!navigator.gpu) {
    throw new Error("This browser does not expose WebGPU");
  }
  await init();
  const source = await loadText("./python/examples/morph_stress_test.py");
  const results = [];

  status.value = "Starting Python/Pyodide authoring worker…";
  const workerStartupStarted = performance.now();
  authoringClient = new PythonAuthoringClient();
  await authoringClient.ready();
  const workerStartupMs = performance.now() - workerStartupStarted;

  status.value = "Priming Noon import and authoring code…";
  const importWarmupStarted = performance.now();
  const importWarmup = await authoringClient.run(source, {
    object_count: AUTHORING_IMPORT_WARMUP_OBJECTS,
  });
  const importWarmupMs = performance.now() - importWarmupStarted;
  assertScene(importWarmup);
  authoringSetup = {
    workerStartupMs,
    importWarmupMs,
    importWarmupObjects: AUTHORING_IMPORT_WARMUP_OBJECTS,
    measuredSamplesPerCase: AUTHORING_SAMPLES,
  };
  setup.value =
    `One-time setup excluded from authoring rows · worker/Pyodide ${formatNumber(workerStartupMs)} ms · ` +
    `first Noon import/source ${formatNumber(importWarmupMs)} ms · ` +
    `${AUTHORING_SAMPLES} measured authoring runs per case`;

  for (const objectCount of CASES) {
    status.value = `Priming ${objectCount.toLocaleString()}-object authoring path…`;
    const caseWarmupStarted = performance.now();
    const caseWarmup = await authoringClient.run(source, { object_count: objectCount });
    const caseWarmupMs = performance.now() - caseWarmupStarted;
    assertScene(caseWarmup);
    await nextAnimationFrame();

    const authoringSamples = new SampleWindow(AUTHORING_SAMPLES);
    let authored = null;
    for (let sample = 0; sample < AUTHORING_SAMPLES; sample += 1) {
      status.value =
        `Authoring ${objectCount.toLocaleString()} objects · sample ${sample + 1}/${AUTHORING_SAMPLES}…`;
      const authoredAt = performance.now();
      const candidate = await authoringClient.run(source, { object_count: objectCount });
      authoringSamples.record(performance.now() - authoredAt);
      assertScene(candidate);
      authored = candidate;
      await nextAnimationFrame();
    }
    const authoringMs = authoringSamples.summary();

    activePlayer?.free?.();
    const createStarted = performance.now();
    activePlayer = await NoonCanvasPlayer.create(
      canvas,
      JSON.stringify(authored.document),
      4.0,
    );
    const playerCreateMs = performance.now() - createStarted;
    resizePlayer(activePlayer);
    activePlayer.setCamera(0, 0, 4.4);
    const gpuTimestampSupported = activePlayer.gpuProfilingSupported();
    activePlayer.setGpuProfilingEnabled(true);
    activePlayer.resetGpuProfiling();

    status.value = `Cold frame · ${objectCount.toLocaleString()} objects…`;
    const coldTimestamp = await nextAnimationFrame();
    const coldPresented = activePlayer.renderFrame(coldTimestamp);
    if (!coldPresented) {
      throw new Error("Cold benchmark frame was not presented");
    }
    const cold = {
      cpuFrameMs: finiteOrNull(activePlayer.lastCpuFrameMs()),
      runtimeMs: finiteOrNull(activePlayer.lastRuntimeEvaluationMs()),
      prepareMs: finiteOrNull(activePlayer.lastFramePrepareMs()),
      uploadMs: finiteOrNull(activePlayer.lastUploadMs()),
      encodeSubmitMs: finiteOrNull(activePlayer.lastEncodeSubmitMs()),
      uploadBytes: activePlayer.lastBytesUploaded(),
      geometryCacheMisses: activePlayer.lastGeometryCacheMisses(),
      drawCalls: activePlayer.lastDrawCalls(),
      instances: activePlayer.lastInstancesDrawn(),
    };

    for (let frame = 0; frame < WARMUP_FRAMES; frame += 1) {
      status.value = `Render warm-up ${frame + 1}/${WARMUP_FRAMES} · ${objectCount.toLocaleString()} objects…`;
      const timestamp = await nextAnimationFrame();
      activePlayer.renderFrame(timestamp);
    }

    activePlayer.resetGpuProfiling();
    const windows = {
      cpuFrame: new SampleWindow(MEASURED_FRAMES),
      runtime: new SampleWindow(MEASURED_FRAMES),
      prepare: new SampleWindow(MEASURED_FRAMES),
      upload: new SampleWindow(MEASURED_FRAMES),
      encode: new SampleWindow(MEASURED_FRAMES),
      interval: new SampleWindow(MEASURED_FRAMES),
    };
    let previousTimestamp = null;

    for (let frame = 0; frame < MEASURED_FRAMES; frame += 1) {
      status.value = `Measuring ${frame + 1}/${MEASURED_FRAMES} · ${objectCount.toLocaleString()} objects…`;
      const timestamp = await nextAnimationFrame();
      const presented = activePlayer.renderFrame(timestamp);
      if (!presented) {
        frame -= 1;
        continue;
      }
      if (previousTimestamp !== null) {
        windows.interval.record(timestamp - previousTimestamp);
      }
      previousTimestamp = timestamp;
      windows.cpuFrame.record(activePlayer.lastCpuFrameMs());
      windows.runtime.record(activePlayer.lastRuntimeEvaluationMs());
      windows.prepare.record(activePlayer.lastFramePrepareMs());
      windows.upload.record(activePlayer.lastUploadMs());
      windows.encode.record(activePlayer.lastEncodeSubmitMs());
    }

    await waitForGpuSamples(activePlayer, gpuTimestampSupported, MEASURED_FRAMES);

    const result = {
      objects: objectCount,
      authoring: {
        samples: AUTHORING_SAMPLES,
        warmupMs: caseWarmupMs,
        endToEndMs: authoringMs,
      },
      playerCreateMs,
      viewport: [canvas.width, canvas.height],
      devicePixelRatio: window.devicePixelRatio || 1,
      cold,
      steady: {
        frames: MEASURED_FRAMES,
        cpuFrameMs: windows.cpuFrame.summary(),
        runtimeMs: windows.runtime.summary(),
        prepareMs: windows.prepare.summary(),
        uploadMs: windows.upload.summary(),
        encodeSubmitMs: windows.encode.summary(),
        frameIntervalMs: windows.interval.summary(),
        drawCalls: activePlayer.lastDrawCalls(),
        instances: activePlayer.lastInstancesDrawn(),
        lastUploadBytes: activePlayer.lastBytesUploaded(),
        geometryCacheMisses: activePlayer.lastGeometryCacheMisses(),
        gpuTimestampSupported,
        gpuRenderMs: gpuTimestampSupported
          ? {
              samples: activePlayer.gpuProfiledFrameCount(),
              dropped: activePlayer.gpuDroppedSampleCount(),
              failed: activePlayer.gpuFailedSampleCount(),
              p50: finiteOrNull(activePlayer.gpuRenderP50Ms()),
              p95: finiteOrNull(activePlayer.gpuRenderP95Ms()),
              last: finiteOrNull(activePlayer.lastGpuRenderMs()),
            }
          : null,
      },
    };
    results.push(result);
    appendResultRow(result);
    jsonOutput.textContent = JSON.stringify(buildReport(results), null, 2);
  }

  const report = buildReport(results);
  window.__NOON_MORPH_BENCHMARK__ = report;
  jsonOutput.textContent = JSON.stringify(report, null, 2);
  status.value = "Benchmark complete · results are also available as window.__NOON_MORPH_BENCHMARK__";
  status.dataset.state = "complete";
  console.log("NOON_MORPH_BENCHMARK", report);
} catch (error) {
  console.error(error);
  status.value = `Benchmark failed: ${error}`;
  status.dataset.state = "error";
} finally {
  authoringClient?.terminate();
}

function buildReport(results) {
  return {
    benchmark: "Noon fixed-topology path morph scaling",
    generatedAt: new Date().toISOString(),
    userAgent: navigator.userAgent,
    authoringSetup,
    renderWarmupFrames: WARMUP_FRAMES,
    measuredRenderFrames: MEASURED_FRAMES,
    cases: results,
  };
}

function appendResultRow(result) {
  const row = document.createElement("tr");
  const values = [
    result.objects.toLocaleString(),
    formatSummary(result.authoring.endToEndMs),
    formatNumber(result.cold.cpuFrameMs),
    formatBytes(result.cold.uploadBytes),
    String(result.cold.geometryCacheMisses),
    String(result.steady.drawCalls),
    formatSummary(result.steady.cpuFrameMs),
    formatSummary(result.steady.runtimeMs),
    formatSummary(result.steady.prepareMs),
    formatSummary(result.steady.uploadMs),
    formatSummary(result.steady.encodeSubmitMs),
    result.steady.gpuRenderMs
      ? `${formatNumber(result.steady.gpuRenderMs.p50)} / ${formatNumber(result.steady.gpuRenderMs.p95)}`
      : "unsupported",
    formatSummary(result.steady.frameIntervalMs),
  ];
  for (const value of values) {
    const cell = document.createElement("td");
    cell.textContent = value;
    row.append(cell);
  }
  resultsBody.append(row);
}

function assertScene(result) {
  if (result.kind !== "scene_document") {
    throw new Error("Morph stress source did not return a Scene");
  }
}

async function waitForGpuSamples(player, supported, measuredFrames) {
  if (!supported) {
    return;
  }
  const deadline = performance.now() + GPU_SETTLE_TIMEOUT_MS;
  while (performance.now() < deadline) {
    const accounted =
      player.gpuProfiledFrameCount() +
      player.gpuDroppedSampleCount() +
      player.gpuFailedSampleCount();
    if (accounted >= measuredFrames) {
      return;
    }
    await nextAnimationFrame();
  }
}

function resizePlayer(player) {
  const scale = window.devicePixelRatio || 1;
  const width = Math.max(1, Math.round(canvas.clientWidth * scale));
  const height = Math.max(1, Math.round(canvas.clientHeight * scale));
  player.resize(width, height);
}

function nextAnimationFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

async function loadText(path) {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`Unable to load ${path}: HTTP ${response.status}`);
  }
  return response.text();
}

function formatSummary(summary) {
  return summary === null
    ? "—"
    : `${formatNumber(summary.p50)} / ${formatNumber(summary.p95)}`;
}

function formatNumber(value) {
  return Number.isFinite(value) ? Number(value).toFixed(2) : "—";
}

function finiteOrNull(value) {
  return Number.isFinite(value) ? value : null;
}

function formatBytes(bytes) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KiB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}
