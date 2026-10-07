import assert from "node:assert/strict";
import { summarizeSamples, summarizeCadence } from "../web/frame-metrics.js";

const TIME_EPSILON_SECONDS = 0.001;
const END_TOLERANCE_SECONDS = 0.001;

// Explicit workloads, using the same source and window on both packages. These
// are test protocols, not an alternative runtime clock or benchmark registry.
export function productMeasurement(exampleId) {
  const common = { version: 5, clock: "renderer-sampled", sampler: "renderer-only",
    preparation: "completed-cold-pass" };
  switch (exampleId) {
    case "parity-square-and-circle":
      return { ...common, sourcePath: "python/examples/manim_parity_square_and_circle.py",
        windowStartSeconds: 1, windowEndSeconds: 4,
        endpointHoldSeconds: 0.5, sourceEndSeconds: 4.5, gapClock: null };
    case "showcase-camera-follows-path":
      return { ...common, sourcePath: "python/examples/showcase_camera_follows_path.py",
        windowStartSeconds: 3.7, windowEndSeconds: 6.9,
        endpointHoldSeconds: 0.4, sourceEndSeconds: 9.8,
        gapClock: "renderer-publication-submission" };
    // Continuous existing-source segments only. The following static hold
    // exposes the exact endpoint, but never contributes to scored FPS.
    case "showcase-first-scene":
      return { ...common, sourcePath: "python/examples/showcase_first_scene.py",
        windowStartSeconds: 2.6, windowEndSeconds: 6.8,
        endpointHoldSeconds: 1.2, sourceEndSeconds: 8,
        gapClock: "renderer-publication-submission" };
    case "showcase-raster-images":
      return { ...common, sourcePath: "python/examples/showcase_raster_images.py",
        windowStartSeconds: 3.4, windowEndSeconds: 6.4,
        endpointHoldSeconds: 1.2, sourceEndSeconds: 7.6,
        gapClock: "renderer-publication-submission" };
    case "showcase-bezier-paths":
      return { ...common, sourcePath: "python/examples/showcase_bezier_paths.py",
        windowStartSeconds: 1.4, windowEndSeconds: 3.4,
        endpointHoldSeconds: 0.7, sourceEndSeconds: 9.5,
        gapClock: "renderer-publication-submission" };
    default:
      throw new Error(`unsupported product measurement example: ${exampleId}`);
  }
}

// Optional existing publication-stage diagnostics supply actual render-call
// timestamps. Never infer per-frame gaps from the 50 ms metrics polling timer.
// Counter coverage fails closed if the bounded worker ring lost any frame.
function presentationWindow(presentationSamples, fps) {
  assert.ok(Array.isArray(presentationSamples), "presentation samples must be an array");
  const epoch = presentationSamples.filter(sample => sample.session === fps.session &&
    sample.clockOriginMs === fps.clockOriginMs);
  assert.ok(epoch.every((sample, index) => Number.isSafeInteger(sample.sequence) && sample.sequence >= 0 &&
    Number.isFinite(sample.presentedAtMs) && sample.presentedAtMs >= 0 &&
    (index === 0 || (sample.sequence > epoch[index - 1].sequence &&
      sample.presentedAtMs > epoch[index - 1].presentedAtMs))),
  "publication presentation timestamps and sequences must advance within one epoch");
  const before = epoch.findLast(sample => sample.presentedAtMs <= fps.startRendererAt);
  const window = [before, ...epoch.filter(sample => sample.presentedAtMs > fps.startRendererAt &&
    sample.presentedAtMs <= fps.endRendererAt)].filter(Boolean);
  assert.ok(Number.isSafeInteger(fps.endFrames - fps.startFrames) && fps.endFrames > fps.startFrames &&
    before !== undefined && window.length === fps.endFrames - fps.startFrames + 1,
  "publication presentation samples must cover every measured renderer frame");
  return window;
}

export function samplePresentationGaps(presentationSamples, fps) {
  const window = presentationWindow(presentationSamples, fps);
  const intervals = window.slice(1).map((sample, index) => sample.presentedAtMs - window[index].presentedAtMs);
  return { clock: "renderer-publication-submission", frameCount: window.length,
    intervalCount: intervals.length, intervalMs: summarizeSamples(intervals),
    cadence: summarizeCadence(intervals, 60) };
}

// CPU wall times cover every scored publication. Renderer counters describe the
// last frame at each diagnostic poll: summarize those snapshots, never sum them
// as though polling captured every upload/draw. No GPU execution time is implied.
export function sampleRendererCosts(presentationSamples, frameSamples, fps) {
  const window = presentationWindow(presentationSamples, fps).slice(1);
  const cpuWallMs = Object.fromEntries(["applyMs", "renderMs", "ackPostMs"].map(key => {
    const values = window.map(sample => sample[key]);
    assert.ok(values.every(value => Number.isFinite(value) && value >= 0),
      `${key}: publication CPU wall time must be finite and nonnegative`);
    return [key, summarizeSamples(values)];
  }));
  const observations = productObservations(frameSamples).filter(sample =>
    sample.session === fps.session && sample.clockOriginMs === fps.clockOriginMs &&
    sample.rendererAt >= fps.startRendererAt && sample.rendererAt <= fps.endRendererAt &&
    sample.ready === true && sample.needsPresent === false && sample.bufferedDeltas === 0);
  const snapshots = observations.filter((sample, index) => index === 0 ||
    sample.frames !== observations[index - 1].frames);
  assert.ok(snapshots.length >= 10 && snapshots[0].frames === fps.startFrames &&
    snapshots.at(-1).frames === fps.endFrames,
  "renderer cost snapshots must cover the scored FPS observation endpoints");
  const fields = ["drawCalls", "instancesDrawn", "bytesUploaded", "geometryCacheMisses",
    "objectCount", "rendererRebuilds", "modeSwitches"];
  for (const key of fields) {
    assert.ok(snapshots.every(sample => Number.isSafeInteger(sample.counters?.[key]) &&
      sample.counters[key] >= 0), `${key}: renderer counter must be a nonnegative safe integer`);
  }
  const lifecycle = Object.fromEntries(["rendererRebuilds", "modeSwitches"].map(key => {
    assert.ok(snapshots.every((sample, index) => index === 0 ||
      sample.counters[key] >= snapshots[index - 1].counters[key]),
    `${key}: renderer lifetime counter must not decrease within one epoch`);
    return [key, snapshots.at(-1).counters[key] - snapshots[0].counters[key]];
  }));
  return { clock: "renderer-publication-submission", frameCount: window.length, cpuWallMs,
    sampledLastFrame: { sampleCount: snapshots.length, fields: Object.fromEntries(
      fields.slice(0, 5).map(key => [key, summarizeSamples(snapshots.map(sample => sample.counters[key]))])) },
    lifecycle };
}

function productObservations(samples) {
  const observations = [];
  let latestMetricAt = -Infinity;
  for (const sample of samples) {
    const owned = sample.phase === "endpoint" || (sample.phase === "source" &&
      sample.runInFlight && sample.playbackControls === "unavailable");
    if (!owned ||
      !Number.isFinite(sample.metricAt) || !Number.isFinite(sample.frames) ||
      !Number.isFinite(sample.time) || sample.metricAt <= latestMetricAt) continue;
    assert.ok(Number.isSafeInteger(sample.frames) && sample.frames >= 0,
      "renderer frame counter must be a nonnegative safe integer");
    if (sample.ready === true && sample.frames > 0) {
      assert.ok(Number.isSafeInteger(sample.session) && sample.session >= 0,
        "renderer session must be a nonnegative safe integer");
      assert.ok(Number.isFinite(sample.clockOriginMs) && sample.clockOriginMs > 0,
        "renderer clock origin must be finite and positive");
      assert.ok(Number.isFinite(sample.rendererAt) && sample.rendererAt >= 0,
        "renderer timestamp must be finite and nonnegative");
    }
    latestMetricAt = sample.metricAt;
    observations.push(sample);
  }
  return observations;
}

function presentationEpochs(observations) {
  const epochs = [];
  for (const observation of observations) {
    const current = epochs.at(-1);
    if (current === undefined || observation.session !== current.at(-1).session ||
      observation.clockOriginMs !== current.at(-1).clockOriginMs ||
      observation.frames < current.at(-1).frames ||
      observation.time + TIME_EPSILON_SECONDS < current.at(-1).time) {
      epochs.push([observation]);
    } else {
      current.push(observation);
    }
  }
  return epochs;
}

// Score a bounded warm authored window in one renderer/session clock. Polling
// and the aggregate source-metrics reply may arrive late; neither is the
// renderer's sampling clock. Retain raw observations, including idle duplicates.
export function sampleRendererFps(frameSamples, authoredSeconds, {
  minMeasurementMs = 1_000, warmupSeconds = 0, maxStartDelaySeconds = 0.5,
} = {}) {
  assert.ok(Number.isFinite(authoredSeconds) && authoredSeconds > 0,
    "authored product measurement duration must be finite and positive");
  assert.ok(Number.isFinite(minMeasurementMs) && minMeasurementMs > 0,
    "minimum measurement duration must be finite and positive");
  assert.ok(Number.isFinite(warmupSeconds) && warmupSeconds >= 0 && warmupSeconds < authoredSeconds,
    "warmupSeconds must be finite, nonnegative, and before the endpoint");
  assert.ok(Number.isFinite(maxStartDelaySeconds) && maxStartDelaySeconds >= 0,
    "maxStartDelaySeconds must be finite and nonnegative");
  const observations = productObservations(frameSamples);
  assert.ok(observations.length >= 10,
    "product run did not expose enough renderer observations");
  const epochs = presentationEpochs(observations);
  const eligible = epochs.map((epoch) => {
    const settled = epoch.filter((sample) => sample.ready === true &&
      sample.needsPresent === false && sample.bufferedDeltas === 0 && sample.frames > 0 &&
      sample.time + TIME_EPSILON_SECONDS >= warmupSeconds &&
      sample.time <= authoredSeconds + END_TOLERANCE_SECONDS);
    // Keep the earliest settled observation for each counter/time pair. A later
    // idle endpoint reply cannot extend the measured animation interval.
    return settled.filter((sample, index) => index === 0 ||
      sample.frames !== settled[index - 1].frames ||
      Math.abs(sample.time - settled[index - 1].time) > TIME_EPSILON_SECONDS);
  }).filter((epoch) => {
    const start = epoch[0];
    const end = epoch.at(-1);
    return epoch.length >= 10 && start !== undefined && end !== undefined &&
      start.time <= warmupSeconds + maxStartDelaySeconds + TIME_EPSILON_SECONDS &&
      Math.abs(end.time - authoredSeconds) <= END_TOLERANCE_SECONDS && end.frames > start.frames;
  });
  assert.ok(eligible.length > 0,
    `no settled renderer epoch covered the authored pass through ${authoredSeconds.toFixed(3)} s; ` +
    `epochs=${JSON.stringify(epochs.map((epoch) => ({
      samples: epoch.length,
      start: { frames: epoch[0].frames, time: epoch[0].time },
      end: { frames: epoch.at(-1).frames, time: epoch.at(-1).time },
    })))}`);
  const epoch = eligible.at(-1);
  const start = epoch[0];
  const end = epoch.at(-1);
  assert.ok(epoch.every((sample, index) => index === 0 ||
    sample.rendererAt > epoch[index - 1].rendererAt),
  "renderer timestamps must advance within one measurement epoch");
  const elapsedMs = end.rendererAt - start.rendererAt;
  assert.ok(elapsedMs >= minMeasurementMs,
    `settled renderer epoch was too short (${elapsedMs.toFixed(0)} ms)`);
  return {
    session: end.session,
    clockOriginMs: end.clockOriginMs,
    startRendererAt: start.rendererAt,
    endRendererAt: end.rendererAt,
    warmupSeconds,
    startFrames: start.frames,
    endFrames: end.frames,
    startTime: start.time,
    endTime: end.time,
    sampleCount: epoch.length,
    observationCount: observations.length,
    epochCount: epochs.length,
    measurementMs: elapsedMs,
    elapsedMs,
    effectiveFps: (end.frames - start.frames) * 1000 / elapsedMs,
  };
}
