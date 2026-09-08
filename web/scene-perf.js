import { PythonAuthoringClient } from "./authoring-client.js";
import { AuthoringExecutionClient } from "./authoring-execution-client.js";
import { BrowserJankMonitor } from "./browser-jank.js";
import { FrameMetrics } from "./frame-metrics.js";

const parameters = new URLSearchParams(location.search);
const sourcePath = parameters.get("source") ?? "./python/demo_scene.py";
if (!sourcePath.startsWith("./python/") || !sourcePath.endsWith(".py")) {
  throw new Error("scene performance source must be a local ./python/*.py file");
}
const warmupFrames = positiveInteger("warmup", 30, 0);
const measuredFrames = positiveInteger("frames", 180);
const targetHz = positiveNumber("targetHz", 60);
const transportMode = parameters.get("transportMode") ?? "transferable";
if (!["transferable", "shared"].includes(transportMode)) throw new Error("unsupported transportMode");
const sharedSlotCapacity = parameters.has("sharedSlotCapacity")
  ? positiveInteger("sharedSlotCapacity") : undefined;
const samples = parameters.get("includeSamples") === "1" ? [] : null;
const context = parseContext(parameters.get("context"));
const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");
const output = document.querySelector("#json");

let client = null;
let execution = null;
let jank = null;
let sourceError = null;
let completedSource = null;
let continuation = false;
let sourceCompleted = false;
let lastSampleTime = 0;
let firstMeasuredTime = null;
let rejectSourceFailure;
const sourceFailure = new Promise((_, reject) => { rejectSourceFailure = reject; });
// Source and execution are independent asynchronous endpoints. Observe failure
// even while an execution request is waiting for a continuation that has failed.
sourceFailure.catch(() => {});
function failSource(error) {
  sourceError = error;
  rejectSourceFailure(error);
}
try {
  const source = await loadText(sourcePath);
  const workerStarted = performance.now();
  client = new PythonAuthoringClient();
  await client.ready();
  const workerStartupMs = performance.now() - workerStarted;
  execution = new AuthoringExecutionClient(canvas, {
    onError: failSource,
  });
  status.value = `Authoring ${sourcePath}…`;
  const authorStarted = performance.now();
  let resolveAttached, rejectAttached;
  const attached = new Promise((resolve, reject) => {
    resolveAttached = resolve;
    rejectAttached = reject;
  });
  let attaching = false;
  async function attach(descriptor, isContinuation) {
    if (attaching) return;
    attaching = true;
    if (!descriptor) throw new Error("scene profiler requires shared semantic execution");
    continuation = isContinuation;
    const ready = await execution.startSemanticExecution(descriptor, {
      authoringClient: client,
      transportMode,
      ...(sharedSlotCapacity === undefined ? {} : { sharedSlotCapacity }),
      ...(continuation ? { pacing: "external_samples" } : { initiallyPaused: true }),
    });
    resolveAttached(ready);
  }
  // Source execution may remain suspended across play/wait. Attach to its
  // existing session, then let exact samples advance Rust's continuation lane.
  void client.run(source, context, {
    onSemanticContinuation: (registration) => attach(registration.semanticExecution, true),
  }).then(async (result) => {
    completedSource = result;
    await attach(result.semanticExecution, false);
  }).catch((error) => {
    failSource(error);
    rejectAttached(error);
  });
  const ready = await Promise.race([attached, sourceFailure]);
  const initialExecutionReadyMs = performance.now() - authorStarted;
  await advanceSample(0);

  // Continue forward through warmup: arbitrary host callbacks cannot be
  // implicitly rewound/replayed to reset a benchmark clock.
  for (let frame = 0; frame < warmupFrames && !sourceCompleted; frame += 1) {
    status.value = `Warm-up ${frame + 1}/${warmupFrames} · ${sourcePath}…`;
    await nextAnimationFrame();
    await advanceSample((frame + 1) / targetHz);
  }
  const before = (await execution.metrics()).metrics;
  const cadence = new FrameMetrics({ targetHz });
  jank = new BrowserJankMonitor();
  const measurementStart = performance.now();
  jank.start();
  for (let frame = 0; frame < measuredFrames && !sourceCompleted; frame += 1) {
    status.value = `Measuring ${frame + 1}/${measuredFrames} · ${sourcePath}…`;
    const timestamp = await nextAnimationFrame();
    const started = performance.now();
    await advanceSample((warmupFrames + frame + 1) / targetHz);
    firstMeasuredTime ??= lastSampleTime;
    const advanceRoundTripMs = performance.now() - started;
    cadence.record(timestamp, advanceRoundTripMs);
    samples?.push({ sceneTime: lastSampleTime, advanceRoundTripMs });
  }
  const measurementEnd = performance.now();
  jank.stop();
  const metrics = (await execution.metrics()).metrics;
  if (sourceError) throw sourceError;
  const frame = cadence.summary();
  const report = {
    schemaVersion: 2,
    ...(samples === null ? {} : { samples }),
    benchmark: "Noon shared authored scene profile",
    generatedAt: new Date().toISOString(),
    scene: { source: sourcePath, context, objects: metrics.objectCount, camera: "authored" },
    environment: {
      userAgent: navigator.userAgent,
      rendererBackend: execution.rendererBackend,
      devicePixelRatio: window.devicePixelRatio || 1,
      viewportCssPixels: [canvas.clientWidth, canvas.clientHeight],
      targetHz,
    },
    setup: { workerStartupMs, initialExecutionReadyMs, warmupFrames },
    execution: {
      mode: execution.mode,
      transportMode: ready.transportMode,
      sourceContinuation: continuation,
      sourceCompleted: completedSource !== null,
      authoredDuration: completedSource?.duration ?? null,
      firstMeasuredTime,
      lastMeasuredTime: lastSampleTime,
      requestedMeasuredFrames: measuredFrames,
      requestedSampleStepSeconds: 1 / targetHz,
    },
    cadence: {
      frames: frame.frames,
      frameIntervalMs: frame.interval,
      effective: frame.cadence,
    },
    pipeline: {
      // This includes worker round trips, callbacks, publication and rendering.
      // It is deliberately not labeled isolated CPU or GPU execution time.
      advanceRoundTripMs: frame.submission,
    },
    renderer: {
      presentedFrames: metrics.presentedFrames - before.presentedFrames,
      drawCalls: metrics.drawCalls,
      instances: metrics.instancesDrawn,
      lastUploadBytes: metrics.bytesUploaded,
      geometryCacheMisses: metrics.geometryCacheMisses,
    },
    browser: { longTasks: jank.summary(measurementStart, measurementEnd) },
    unavailableMetrics: ["cpuFrameP95Ms", "gpuP95Ms"],
  };
  window.__NOON_SCENE_PERF__ = report;
  output.textContent = JSON.stringify(report, null, 2);
  status.value = `Complete · ${sourcePath} · ${format(report.cadence.effective?.effectiveFps)} FPS · ` +
    `p95 ${format(report.pipeline.advanceRoundTripMs?.p95)} ms advance round trip`;
  status.dataset.state = "complete";
  console.log("NOON_SCENE_PERF", report);
} catch (error) {
  console.error(error);
  status.value = `Scene profile failed: ${error}`;
  status.dataset.state = "error";
} finally {
  jank?.stop();
  execution?.terminate();
  client?.terminate();
}

async function advanceSample(time) {
  if (sourceError) throw sourceError;
  const request = continuation
    ? execution.sampleToAuthoredTime(time, { stopAtSourceCompletion: true })
    : execution.advanceTo(time);
  const result = await Promise.race([request, sourceFailure]);
  if (sourceError) throw sourceError;
  sourceCompleted = result.sourceCompleted === true;
  lastSampleTime = result.time;
  return result;
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

function positiveInteger(name, fallback, minimum = 1) {
  const value = parameters.get(name);
  if (value === null) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < minimum) throw new Error(`${name} must be an integer >= ${minimum}`);
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
