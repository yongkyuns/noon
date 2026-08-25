import init, { NoonCanvasPlayer } from "./pkg/noon_web.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { BrowserJankMonitor } from "./browser-jank.js";
import { summarizeSamples } from "./frame-metrics.js";
import { SceneIdentityMap } from "./scene-identity.js";

const parameters = new URLSearchParams(location.search);
const objectCount = positiveInteger("objects", 1_000);
const samples = positiveInteger("samples", 5);
const scrubSamples = positiveInteger("scrubs", 20);
const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");
const output = document.querySelector("#json");

let client = null;
let player = null;
const identities = new SceneIdentityMap();
const jank = new BrowserJankMonitor();

try {
  await init();
  const source = await loadText("./python/examples/authoring_perf_scene.py");
  jank.start();

  const cold = await coldRun(source);
  const unchanged = [];
  for (let sample = 0; sample < samples; sample += 1) {
    status.value = `Warm unchanged rerun ${sample + 1}/${samples} · ${objectCount.toLocaleString()} objects…`;
    unchanged.push(await rerun(source, 0, "unchanged"));
  }

  const localEdit = [];
  let variant = 1;
  for (let sample = 0; sample < samples; sample += 1) {
    status.value = `One-object edit ${sample + 1}/${samples} · ${objectCount.toLocaleString()} objects…`;
    localEdit.push(await rerun(source, variant, "one-object-style"));
    variant = variant === 0 ? 1 : 0;
  }

  status.value = `Scrub/seek profile · ${scrubSamples} samples…`;
  const scrubs = profileScrubs(scrubSamples);

  const report = {
    schemaVersion: 1,
    benchmark: "Noon interactive authoring time-to-visible profile",
    generatedAt: new Date().toISOString(),
    environment: {
      userAgent: navigator.userAgent,
      rendererBackend: player.rendererBackend(),
      devicePixelRatio: window.devicePixelRatio || 1,
      viewport: [canvas.width, canvas.height],
      hardwareConcurrency: navigator.hardwareConcurrency ?? null,
    },
    workload: {
      objects: objectCount,
      warmSamples: samples,
      scrubSamples,
      source: "python/examples/authoring_perf_scene.py",
      localEdit: "one stable-identity circle changes fill color",
    },
    cold,
    warmUnchanged: summarizeOperations(unchanged),
    oneObjectEdit: summarizeOperations(localEdit),
    scrub: summarizeScrubs(scrubs),
  };

  window.__NOON_AUTHORING_PERF__ = report;
  output.textContent = JSON.stringify(report, null, 2);
  status.value =
    `Complete · unchanged visible p95 ${format(report.warmUnchanged.timeToVisibleMs?.p95)} ms · ` +
    `one-object edit p95 ${format(report.oneObjectEdit.timeToVisibleMs?.p95)} ms`;
  status.dataset.state = "complete";
  console.log("NOON_AUTHORING_PERF", report);
} catch (error) {
  console.error(error);
  status.value = `Authoring benchmark failed: ${error}`;
  status.dataset.state = "error";
} finally {
  jank.stop();
  client?.terminate();
}

async function coldRun(source) {
  status.value = `Cold Run · ${objectCount.toLocaleString()} objects…`;
  const operationStarted = performance.now();

  const workerStarted = performance.now();
  client = new PythonAuthoringClient();
  await client.ready();
  const workerStartupMs = performance.now() - workerStarted;

  const authored = await author(source, 0);
  const stabilized = stabilize(authored.result);
  const encoded = serialize(stabilized.document);

  const createStarted = performance.now();
  player = await NoonCanvasPlayer.create(canvas, encoded.json, 4.0);
  const playerCreateMs = performance.now() - createStarted;
  player.resize(canvas.width, canvas.height);
  player.setCamera(0, 0, cameraHeight(objectCount));

  const visible = await presentNextFrame();
  const operationEnded = performance.now();
  return {
    timeToVisibleMs: operationEnded - operationStarted,
    workerStartupMs,
    workerRoundTripMs: authored.ms,
    stabilizeMs: stabilized.ms,
    serializeMs: encoded.ms,
    serializedBytes: encoded.bytes,
    playerCreateMs,
    visibleFrameWaitAndSubmitMs: visible.waitAndSubmitMs,
    frame: visible.frame,
    longTasks: jank.summary(operationStarted, operationEnded),
  };
}

async function rerun(source, variant, kind) {
  const operationStarted = performance.now();
  const authored = await author(source, variant);
  const stabilized = stabilize(authored.result);
  const encoded = serialize(stabilized.document);

  const reconcileStarted = performance.now();
  const playheadBefore = player.time();
  const incremental = player.reconcileScene(encoded.json);
  const reconcileMs = performance.now() - reconcileStarted;
  if (player.time() !== playheadBefore) {
    throw new Error(`${kind} reconciliation changed the current playhead`);
  }

  const visible = await presentNextFrame();
  const operationEnded = performance.now();
  return {
    kind,
    timeToVisibleMs: operationEnded - operationStarted,
    workerRoundTripMs: authored.ms,
    stabilizeMs: stabilized.ms,
    serializeMs: encoded.ms,
    serializedBytes: encoded.bytes,
    reconcileMs,
    incremental,
    visibleFrameWaitAndSubmitMs: visible.waitAndSubmitMs,
    frame: visible.frame,
    longTasks: jank.summary(operationStarted, operationEnded),
  };
}

async function author(source, variant) {
  const started = performance.now();
  const result = await client.run(source, { object_count: objectCount, variant });
  const ms = performance.now() - started;
  if (result.kind !== "scene_document") {
    throw new Error("authoring performance source did not return a Scene");
  }
  return { result, ms };
}

function stabilize(result) {
  const started = performance.now();
  const document = identities.stabilize(result.document, result.identities);
  return { document, ms: performance.now() - started };
}

function serialize(document) {
  const started = performance.now();
  const json = JSON.stringify(document);
  return {
    json,
    ms: performance.now() - started,
    bytes: new TextEncoder().encode(json).byteLength,
  };
}

async function presentNextFrame() {
  const started = performance.now();
  for (let attempt = 0; attempt < 8; attempt += 1) {
    const timestamp = await nextAnimationFrame();
    const submitStarted = performance.now();
    const presented = player.renderFrame(timestamp);
    const browserSubmitMs = performance.now() - submitStarted;
    if (presented) {
      return {
        waitAndSubmitMs: performance.now() - started,
        frame: frameSnapshot(browserSubmitMs),
      };
    }
  }
  throw new Error("authoring result did not present within eight animation frames");
}

function profileScrubs(count) {
  const results = [];
  for (let index = 0; index < count; index += 1) {
    const target = ((index * 0.61803398875) % 1) * 3.8;
    player.resetClock();
    player.renderFrame(0);
    const started = performance.now();
    const presented = player.renderFrame(target * 1000);
    const elapsed = performance.now() - started;
    if (!presented) {
      throw new Error(`scrub frame at ${target.toFixed(3)}s was not presented`);
    }
    results.push({
      targetSeconds: target,
      timeToVisibleMs: elapsed,
      frame: frameSnapshot(elapsed),
    });
  }
  return results;
}

function frameSnapshot(browserSubmitMs) {
  return {
    browserSubmitMs,
    cpuFrameMs: finite(player.lastCpuFrameMs()),
    runtimeMs: finite(player.lastRuntimeEvaluationMs()),
    prepareMs: finite(player.lastFramePrepareMs()),
    uploadMs: finite(player.lastUploadMs()),
    encodeSubmitMs: finite(player.lastEncodeSubmitMs()),
    uploadBytes: player.lastBytesUploaded(),
    drawCalls: player.lastDrawCalls(),
    instances: player.lastInstancesDrawn(),
    geometryCacheMisses: player.lastGeometryCacheMisses(),
  };
}

function summarizeOperations(operations) {
  return {
    samples: operations.length,
    incrementalCount: operations.filter(({ incremental }) => incremental).length,
    timeToVisibleMs: summary(operations, "timeToVisibleMs"),
    workerRoundTripMs: summary(operations, "workerRoundTripMs"),
    stabilizeMs: summary(operations, "stabilizeMs"),
    serializeMs: summary(operations, "serializeMs"),
    serializedBytes: summary(operations, "serializedBytes"),
    reconcileMs: summary(operations, "reconcileMs"),
    visibleFrameWaitAndSubmitMs: summary(operations, "visibleFrameWaitAndSubmitMs"),
    frameCpuMs: summarizeSamples(operations.map(({ frame }) => frame.cpuFrameMs)),
    framePrepareMs: summarizeSamples(operations.map(({ frame }) => frame.prepareMs)),
    frameUploadMs: summarizeSamples(operations.map(({ frame }) => frame.uploadMs)),
    frameEncodeSubmitMs: summarizeSamples(operations.map(({ frame }) => frame.encodeSubmitMs)),
    uploadBytes: summarizeSamples(operations.map(({ frame }) => frame.uploadBytes)),
    longTaskCount: operations.reduce(
      (sum, operation) => sum + (operation.longTasks.supported ? operation.longTasks.count : 0),
      0,
    ),
  };
}

function summarizeScrubs(scrubs) {
  return {
    samples: scrubs.length,
    timeToVisibleMs: summary(scrubs, "timeToVisibleMs"),
    runtimeMs: summarizeSamples(scrubs.map(({ frame }) => frame.runtimeMs)),
    prepareMs: summarizeSamples(scrubs.map(({ frame }) => frame.prepareMs)),
    uploadMs: summarizeSamples(scrubs.map(({ frame }) => frame.uploadMs)),
    encodeSubmitMs: summarizeSamples(scrubs.map(({ frame }) => frame.encodeSubmitMs)),
    uploadBytes: summarizeSamples(scrubs.map(({ frame }) => frame.uploadBytes)),
  };
}

function summary(values, field) {
  return summarizeSamples(values.map((value) => value[field]));
}

function cameraHeight(count) {
  const columns = Math.ceil(Math.sqrt(count * (16 / 9)));
  const rows = Math.ceil(count / columns);
  return Math.max(4.5, rows * 0.095);
}

async function loadText(path) {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`Unable to load ${path}: HTTP ${response.status}`);
  }
  return response.text();
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

function finite(value) {
  return Number.isFinite(value) ? value : 0;
}

function format(value) {
  return Number.isFinite(value) ? value.toFixed(2) : "—";
}
