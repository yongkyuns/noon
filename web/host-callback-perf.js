import { PythonAuthoringClient } from "./authoring-client.js";
import { AuthoringExecutionClient } from "./authoring-execution-client.js";
import { BrowserJankMonitor } from "./browser-jank.js";
import { FrameMetrics, SampleWindow } from "./frame-metrics.js";

const SAMPLE_RATE_HZ = 60;
const SAMPLE_STEP_SECONDS = 1 / SAMPLE_RATE_HZ;
const parameters = new URLSearchParams(location.search);
const objectCount = positiveInteger("objects", 1_000);
const activeCount = positiveInteger("active", 1);
const warmupFrames = positiveInteger("warmup", 30);
const measuredFrames = positiveInteger("frames", 300);
if (activeCount > objectCount) throw new Error("active object count cannot exceed object count");

const status = document.querySelector("#status");
const output = document.querySelector("#json");
let authoring = null;

try {
  authoring = new PythonAuthoringClient();
  await authoring.ready();

  // Leave enough authored extent for startup ticks plus every exact benchmark
  // sample. The endpoint, rather than this harness, remains the execution clock.
  const workloadDurationSeconds =
    (warmupFrames + measuredFrames) * SAMPLE_STEP_SECONDS + 2;
  status.value = `Native timeline · ${activeCount}/${objectCount} active…`;
  const nativeResult = await authorAndMeasure(
    nativeSource(objectCount, activeCount, workloadDurationSeconds),
    workloadDurationSeconds,
    false,
  );
  status.value = `Python updater · ${activeCount}/${objectCount} active…`;
  const hostResult = await authorAndMeasure(
    hostSource(objectCount, activeCount, workloadDurationSeconds),
    workloadDurationSeconds,
    true,
  );

  const report = {
    schemaVersion: 2,
    benchmark: "Noon canonical native timeline versus Python callback advance round trip",
    generatedAt: new Date().toISOString(),
    workload: {
      objects: objectCount,
      active: activeCount,
      warmupFrames,
      measuredFrames,
      authoredSampleRateHz: SAMPLE_RATE_HZ,
      semantics: "the first active objects translate right at 0.1 authored units per second",
      layout:
        "all objects remain coincident and visible, matching the previous benchmark workload",
    },
    environment: {
      userAgent: navigator.userAgent,
      hardwareConcurrency: navigator.hardwareConcurrency ?? null,
      devicePixelRatio: window.devicePixelRatio || 1,
    },
    native: nativeResult,
    host: hostResult,
    overhead: {
      advanceRoundTripP95Ms: difference(
        hostResult.advanceRoundTripMs?.p95,
        nativeResult.advanceRoundTripMs?.p95,
      ),
      cadenceP95Ms: difference(
        hostResult.frameIntervalMs?.p95,
        nativeResult.frameIntervalMs?.p95,
      ),
      lastPublicationUploadBytes: difference(
        hostResult.locality.lastPublication.bytesUploaded,
        nativeResult.locality.lastPublication.bytesUploaded,
      ),
    },
  };
  window.__NOON_HOST_CALLBACK_PERF__ = report;
  output.textContent = JSON.stringify(report, null, 2);
  status.value =
    `Complete · native round trip p95 ${format(nativeResult.advanceRoundTripMs?.p95)} ms · ` +
    `host ${format(hostResult.advanceRoundTripMs?.p95)} ms`;
  status.dataset.state = "complete";
  console.log("NOON_HOST_CALLBACK_PERF", report);
} catch (error) {
  console.error(error);
  status.value = `Host callback profile failed: ${error}`;
  status.dataset.state = "error";
} finally {
  authoring?.terminate();
}

async function author(source) {
  const result = await authoring.run(source);
  if (result.kind !== "scene_document" || result.semanticExecution === null) {
    throw new Error("performance source did not return canonical semantic execution");
  }
  return result;
}

async function authorAndMeasure(source, workloadDurationSeconds, expectsCallbacks) {
  const authored = await author(source);
  try {
    const hasCallbacks = authored.semanticExecution.callbackSessionId !== undefined;
    if (hasCallbacks !== expectsCallbacks) {
      throw new Error(
        expectsCallbacks
          ? "host comparison scene did not register callbacks"
          : "native comparison scene unexpectedly registered callbacks",
      );
    }
    return await measureWorkload(authored, workloadDurationSeconds);
  } finally {
    await authoring.releaseSemanticExecution(authored.semanticExecution.contextId);
  }
}

async function measureWorkload(authored, workloadDurationSeconds) {
  const canvas = document.createElement("canvas");
  canvas.width = 320;
  canvas.height = 180;
  canvas.style.cssText =
    "position:fixed;left:-10000px;top:0;width:320px;height:180px;pointer-events:none";
  document.body.append(canvas);
  const execution = new AuthoringExecutionClient(canvas);
  try {
    const ready = await execution.startSemanticExecution(authored.semanticExecution, {
      authoringClient: authoring,
      loopDurationSeconds: workloadDurationSeconds,
      transportMode: "transferable",
    });
    const started = await execution.state();
    const paused = await execution.pause();
    if (paused.playing !== false || !Number.isFinite(paused.time)) {
      throw new Error("canonical performance endpoint did not pause coherently");
    }

    // Start on the next exact 60 Hz authored-time boundary after any startup
    // presentation. RAF paces samples only and never determines authored time.
    const firstSampleIndex = Math.floor(paused.time * SAMPLE_RATE_HZ + 1e-9) + 1;
    const sampleTime = (index) => (firstSampleIndex + index) * SAMPLE_STEP_SECONDS;
    const finalTime = sampleTime(warmupFrames + measuredFrames - 1);
    if (finalTime > workloadDurationSeconds) {
      throw new Error("performance sample range exceeds the authored workload extent");
    }

    for (let frame = 0; frame < warmupFrames; frame += 1) {
      await execution.advanceTo(sampleTime(frame));
    }
    const metricsBefore = rendererMetrics(await execution.metrics());
    const cadence = new FrameMetrics({ targetHz: SAMPLE_RATE_HZ });
    const roundTrip = new SampleWindow(measuredFrames);
    const jank = new BrowserJankMonitor();
    const start = performance.now();
    jank.start();
    for (let frame = 0; frame < measuredFrames; frame += 1) {
      const timestamp = await nextAnimationFrame();
      const began = performance.now();
      const requestedTime = sampleTime(warmupFrames + frame);
      const advanced = await execution.advanceTo(requestedTime);
      if (advanced.playing !== false || Math.abs(advanced.time - requestedTime) > 1e-9) {
        throw new Error(`canonical endpoint did not publish exact time ${requestedTime}`);
      }
      const elapsed = performance.now() - began;
      roundTrip.record(elapsed);
      cadence.record(timestamp, elapsed);
    }
    const end = performance.now();
    jank.stop();
    const metricsAfter = rendererMetrics(await execution.metrics());
    const finalState = await execution.state();
    const frame = cadence.summary();
    return {
      executionMode: execution.mode,
      rendererBackend: execution.rendererBackend,
      transportMode: ready.transportMode,
      startup: {
        beforePause: { time: started.time, playing: started.playing },
        afterPause: { time: paused.time, playing: paused.playing },
        firstExactSampleIndex: firstSampleIndex,
      },
      authoredSamples: {
        warmupFirst: sampleTime(0),
        measuredFirst: sampleTime(warmupFrames),
        last: finalTime,
        stepSeconds: SAMPLE_STEP_SECONDS,
      },
      advanceRoundTripMs: roundTrip.summary(),
      frameIntervalMs: frame.interval,
      cadence: frame.cadence,
      longTasks: jank.summary(start, end),
      locality: localitySummary(metricsBefore, metricsAfter),
      finalState: {
        time: finalState.time,
        playing: finalState.playing,
      },
    };
  } finally {
    execution.terminate();
    canvas.remove();
  }
}

function localitySummary(before, after) {
  const beforePresented = finiteCounter(before.presentedFrames, "starting presented frame count");
  const afterPresented = finiteCounter(after.presentedFrames, "ending presented frame count");
  const presentedFrames = afterPresented - beforePresented;
  if (presentedFrames !== measuredFrames) {
    throw new Error(
      `exact advances presented ${presentedFrames} frames instead of ${measuredFrames}`,
    );
  }
  const objectCountAfter = finiteCounter(after.objectCount, "renderer object count");
  if (objectCountAfter !== objectCount) {
    throw new Error(`renderer object count ${objectCountAfter} did not match ${objectCount}`);
  }
  return {
    presentedFrames,
    lastPublication: {
      objectCount: objectCountAfter,
      drawCalls: finiteCounter(after.drawCalls, "draw call count"),
      instancesDrawn: finiteCounter(after.instancesDrawn, "instance count"),
      bytesUploaded: finiteCounter(after.bytesUploaded, "uploaded byte count"),
      geometryCacheMisses: finiteCounter(after.geometryCacheMisses, "geometry cache miss count"),
      bufferedDeltas: finiteCounter(after.bufferedDeltas, "buffered delta count"),
      needsPresent: Boolean(after.needsPresent),
    },
  };
}

function rendererMetrics(report) {
  if (
    report === null ||
    typeof report !== "object" ||
    report.metrics === null ||
    typeof report.metrics !== "object"
  ) {
    throw new Error("canonical execution returned an invalid renderer metrics envelope");
  }
  return report.metrics;
}

function nativeSource(objects, active, duration) {
  return `
from noon import Circle, RIGHT, Scene, linear
scene = Scene()
dots = [Circle(0.1) for _ in range(${objects})]
for index, dot in enumerate(dots):
    scene.add(dot, key=f"dot.{index}")
progress = scene.value_tracker(0.0)
for dot in dots[:${active}]:
    scene.bind_position(dot, progress, direction=RIGHT * 0.1)
scene.play(progress.animate(run_time=${duration}, rate_func=linear).set_value(${duration}))
live = scene.live_execution(duration=${duration})
live.evaluate(0.0)
result = scene
`;
}

function hostSource(objects, active, duration) {
  return `
from noon import Circle, RIGHT, Scene
scene = Scene()
dots = [Circle(0.1) for _ in range(${objects})]
for index, dot in enumerate(dots):
    scene.add(dot, key=f"dot.{index}")

def move(mobject, dt):
    mobject.shift(RIGHT * (0.1 * dt))

for dot in dots[:${active}]:
    dot.add_updater(move)
live = scene.live_execution(duration=${duration})
assert live.wait(${duration}) == ${duration}
result = scene
`;
}

function nextAnimationFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function finiteCounter(value, name) {
  if (!Number.isFinite(value) || value < 0) throw new Error(`${name} must be finite and non-negative`);
  return value;
}

function positiveInteger(name, fallback) {
  const value = parameters.get(name);
  if (value === null) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`);
  return parsed;
}

function difference(left, right) {
  return Number.isFinite(left) && Number.isFinite(right) ? left - right : null;
}

function format(value) {
  return Number.isFinite(value) ? Number(value).toFixed(3) : "—";
}
