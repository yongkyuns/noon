// Qualification metadata only. This never drives scene time or changes a demo.
import assert from "node:assert/strict";

const TIME_EPSILON = 1e-7;

export function validateCaptureIntervals(entry) {
  assert.ok(Number.isFinite(entry.duration) && entry.duration > 0, `${entry.id}: invalid duration`);
  assert.ok(Array.isArray(entry.still_intervals), `${entry.id}: declare still intervals explicitly`);
  let previousEnd = -Infinity;
  for (const interval of entry.still_intervals) {
    assert.ok(Array.isArray(interval) && interval.length === 2, `${entry.id}: invalid still interval`);
    const [start, end] = interval;
    assert.ok(Number.isFinite(start) && Number.isFinite(end) && start >= 0 && end > start &&
      end <= entry.duration && start >= previousEnd, `${entry.id}: unordered, overlapping, or out-of-range still interval`);
    previousEnd = end;
  }
}

export function assertCaptureTime(entry, sample, requestedTime) {
  assert.ok(Number.isFinite(requestedTime) && requestedTime >= 0 && requestedTime <= entry.duration,
    `${entry.id}: requested time outside the scene`);
  assert.equal(sample.requestedTime, requestedTime, `${entry.id}: wrong sample request`);
  const published = sample.publishedTime;
  assert.ok(Number.isFinite(published) && published >= 0, `${entry.id}: invalid published time`);
  // Adjacent discrete visibility intervals meet at a threshold. The later
  // interval wins there: an image from before that visibility change is stale.
  let earliest = requestedTime;
  for (const [start, end] of entry.still_intervals) {
    if (requestedTime + TIME_EPSILON >= start && requestedTime <= end + TIME_EPSILON) earliest = start;
  }
  assert.ok(published + TIME_EPSILON >= earliest && published <= requestedTime + TIME_EPSILON,
    `${entry.id}@${requestedTime}: stale/future publication ${published}; expected [${earliest}, ${requestedTime}]`);
  return earliest;
}

// This is a source-completion probe, not a frame relabeled at a rounded time.
// It advances at most 1 ns beyond the decimal storyboard duration and asks the
// existing runtime to stop at the actual endpoint. Completion must be explicit.
export const COMPLETION_PROBE_SLOP = 1e-9;

export function captureSchedule(entry, sampleHz = 30) {
  validateCaptureIntervals(entry);
  assert.ok(Number.isSafeInteger(sampleHz) && sampleHz > 0 && sampleHz <= 240);
  const end = entry.duration;
  const completionTime = end + COMPLETION_PROBE_SLOP;
  const nearEnd = (time) => time >= end - COMPLETION_PROBE_SLOP;
  const authoredSamples = [entry.thumbnail_time, ...entry.beats.map(beat => beat.time)];
  assert.ok(authoredSamples.every(time => Number.isFinite(time) && time > 0 && time <= end));
  const wanted = [...new Set([...authoredSamples.filter(time => !nearEnd(time)), completionTime])].sort((a, b) => a - b);
  const regular = Array.from({ length: Math.ceil(end * sampleHz) }, (_, frame) => frame / sampleHz)
    .filter(time => !nearEnd(time));
  const frameTimes = [...new Set([...regular, ...wanted])].sort((a, b) => a - b);
  return { frameTimes, wanted, completionTime, posterTime: nearEnd(entry.thumbnail_time) ? completionTime : entry.thumbnail_time };
}

export function assertCompletedCapture(entry, sample, requestedTime) {
  assert.equal(requestedTime, entry.duration + COMPLETION_PROBE_SLOP);
  assert.equal(sample.requestedTime, requestedTime, `${entry.id}: wrong completion probe`);
  assert.equal(sample.sourceCompleted, true, `${entry.id}: source did not complete`);
  assert.equal(sample.sourceState, "completed", `${entry.id}: authoring result did not finish`);
  assert.ok(Number.isFinite(sample.authoredDuration) &&
    Math.abs(sample.authoredDuration - entry.duration) <= COMPLETION_PROBE_SLOP,
  `${entry.id}: actual duration ${sample.authoredDuration} differs from storyboard ${entry.duration}`);
  assert.ok(Number.isFinite(sample.publishedTime) &&
    Math.abs(sample.publishedTime - sample.authoredDuration) <= COMPLETION_PROBE_SLOP,
  `${entry.id}: completion frame is not at its actual source endpoint`);
}
