import { PythonAuthoringClient } from "./authoring-client.js";
import { AuthoringExecutionClient } from "./authoring-execution-client.js";
import { BrowserJankMonitor } from "./browser-jank.js";
import { summarizeSamples } from "./frame-metrics.js";

const parameters = new URLSearchParams(location.search);
const objectCount = positiveInteger("objects", 1_000);
const samples = positiveInteger("samples", 5);
const scrubSamples = positiveInteger("scrubs", 20);
const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");
const output = document.querySelector("#json");
const jank = new BrowserJankMonitor();
let client = null;
let execution = null;
let executionError = null;

try {
  const response = await fetch("./python/examples/authoring_perf_scene.py");
  if (!response.ok) throw new Error(`Unable to load authoring source: HTTP ${response.status}`);
  const source = await response.text();
  jank.start();
  const started = performance.now();
  client = new PythonAuthoringClient();
  await client.ready();
  const workerStartupMs = performance.now() - started;
  execution = new AuthoringExecutionClient(canvas, {
    onError: (error) => { executionError = error; },
  });
  const cold = await runSource(source, 0, true);
  cold.workerStartupMs = workerStartupMs;
  cold.totalRoundTripMs += workerStartupMs;
  const unchanged = [];
  const sourceEdits = [];
  for (let sample = 0; sample < samples; sample += 1) {
    status.value = `Unchanged source rerun ${sample + 1}/${samples}…`;
    unchanged.push(await runSource(source, 0));
  }
  for (let sample = 0; sample < samples; sample += 1) {
    status.value = `One-object source edit ${sample + 1}/${samples}…`;
    sourceEdits.push(await runSource(source, sample % 2 === 0 ? 1 : 0));
  }
  const scrubs = [];
  for (let index = 0; index < scrubSamples; index += 1) {
    status.value = `Static scene seek ${index + 1}/${scrubSamples}…`;
    const target = ((index * 0.61803398875) % 1) * 3.8;
    const seekStarted = performance.now();
    const state = await execution.seek(target);
    if (Math.abs(state.time - target) > 1e-5) throw new Error("seek did not reach its target");
    // This is a control round trip, not an isolated GPU or presentation timer.
    scrubs.push(performance.now() - seekStarted);
  }
  const metrics = (await execution.metrics()).metrics;
  if (executionError) throw executionError;
  const report = {
    schemaVersion: 2,
    benchmark: "Noon shared authoring round-trip profile",
    generatedAt: new Date().toISOString(),
    environment: {
      userAgent: navigator.userAgent,
      rendererBackend: execution.rendererBackend,
      viewportCssPixels: [canvas.clientWidth, canvas.clientHeight],
    },
    workload: {
      objects: objectCount,
      warmSamples: samples,
      scrubSamples,
      source: "python/examples/authoring_perf_scene.py",
      sourceEdit: "one circle changes fill color in a rebuilt semantic session",
      camera: "authored",
      seek: "static scene; this does not qualify animated seek parity",
    },
    execution: { mode: execution.mode, rerun: "session-replacement" },
    cold,
    warmUnchanged: summarizeOperations(unchanged),
    oneObjectSourceEdit: summarizeOperations(sourceEdits),
    scrub: { samples: scrubs.length, controlRoundTripMs: summarizeSamples(scrubs) },
    renderer: {
      objectCount: metrics.objectCount,
      drawCalls: metrics.drawCalls,
      instances: metrics.instancesDrawn,
    },
    unavailableMetrics: ["incrementalMutationLatency", "isolatedCpuTime", "gpuTime", "cameraUniformUpdateLatency"],
  };
  output.textContent = JSON.stringify(report, null, 2);
  window.__NOON_AUTHORING_PERF__ = report;
  status.value = `Complete · unchanged rerun p95 ${format(report.warmUnchanged.totalRoundTripMs.p95)} ms · ` +
    `source edit p95 ${format(report.oneObjectSourceEdit.totalRoundTripMs.p95)} ms`;
  status.dataset.state = "complete";
} catch (error) {
  console.error(error);
  status.value = `Authoring benchmark failed: ${error}`;
  status.dataset.state = "error";
} finally {
  jank.stop();
  execution?.terminate();
  client?.terminate();
}

async function runSource(source, variant, cold = false) {
  status.value = cold ? `Cold authoring · ${objectCount} objects…` : status.value;
  const started = performance.now();
  const result = await client.run(source, { object_count: objectCount, variant });
  const authoringRoundTripMs = performance.now() - started;
  if (!result.semanticExecution || result.semanticExecution.continuationGeneration != null) {
    throw new Error("authoring benchmark requires a completed shared static scene");
  }
  const attachStarted = performance.now();
  if (cold) {
    await execution.startSemanticExecution(result.semanticExecution, {
      authoringClient: client, initiallyPaused: true, loopDurationSeconds: 4,
    });
  } else {
    await execution.reconcileSemanticExecution(result.semanticExecution, { authoringClient: client });
    await execution.pause();
  }
  await execution.advanceTo(0);
  const ended = performance.now();
  if (executionError) throw executionError;
  return {
    totalRoundTripMs: ended - started,
    authoringRoundTripMs,
    attachAndPresentRoundTripMs: ended - attachStarted,
    rebuilt: !cold,
    longTasks: jank.summary(started, ended),
  };
}

function summarizeOperations(operations) {
  return {
    samples: operations.length,
    rebuiltCount: operations.filter(({ rebuilt }) => rebuilt).length,
    ...Object.fromEntries(
      ["totalRoundTripMs", "authoringRoundTripMs", "attachAndPresentRoundTripMs"].map(
        (field) => [field, summarizeSamples(operations.map((operation) => operation[field]))],
      ),
    ),
  };
}

function positiveInteger(name, fallback) {
  const value = parameters.get(name);
  if (value === null) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`);
  return parsed;
}

function format(value) {
  return Number.isFinite(value) ? value.toFixed(2) : "—";
}
