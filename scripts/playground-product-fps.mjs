import assert from "node:assert/strict";

const TIME_EPSILON_SECONDS = 0.001;
const END_TOLERANCE_SECONDS = 0.001;

function productObservations(samples) {
  const observations = [];
  let latestMetricAt = -Infinity;
  for (const sample of samples) {
    const owned = sample.phase === "endpoint" || (sample.phase === "source" &&
      sample.runInFlight && sample.playbackControls === "unavailable");
    if (!owned ||
      !Number.isFinite(sample.metricAt) || !Number.isFinite(sample.frames) ||
      !Number.isFinite(sample.time) || sample.metricAt <= latestMetricAt) continue;
    latestMetricAt = sample.metricAt;
    observations.push(sample);
  }
  return observations;
}

function presentationEpochs(observations) {
  const epochs = [];
  for (const observation of observations) {
    const current = epochs.at(-1);
    if (current === undefined || observation.frames < current.at(-1).frames ||
      observation.time + TIME_EPSILON_SECONDS < current.at(-1).time) {
      epochs.push([observation]);
    } else {
      current.push(observation);
    }
  }
  return epochs;
}

// A source-continuation handoff can rebuild the render engine, resetting both
// its frame counter and its authored clock. Score only one settled epoch that
// ends at the authored endpoint; retain every raw sample and observed start
// time rather than claiming that polling observes the exact first frame.
export function sampleRendererFps(frameSamples, authoredSeconds, { minMeasurementMs = 1_000 } = {}) {
  assert.ok(Number.isFinite(authoredSeconds) && authoredSeconds > 0,
    "authored product measurement duration must be finite and positive");
  const observations = productObservations(frameSamples);
  assert.ok(observations.length >= 10,
    "product run did not expose enough renderer observations");
  const epochs = presentationEpochs(observations);
  const eligible = epochs.map((epoch) => epoch.filter((sample) =>
    sample.ready === true && sample.needsPresent === false && sample.bufferedDeltas === 0,
  )).filter((epoch) => {
    const start = epoch[0];
    const end = epoch.at(-1);
    return epoch.length >= 10 && start !== undefined && end !== undefined &&
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
  const elapsedMs = end.now - start.now;
  assert.ok(elapsedMs >= minMeasurementMs,
    `settled renderer epoch was too short (${elapsedMs.toFixed(0)} ms)`);
  return {
    startFrames: start.frames,
    endFrames: end.frames,
    startTime: start.time,
    endTime: end.time,
    sampleCount: epoch.length,
    observationCount: observations.length,
    epochCount: epochs.length,
    measurementMs: elapsedMs,
    elapsedMs,
    effectiveFps: (end.frames - start.frames) / Math.max(elapsedMs / 1000, 0.001),
  };
}
