import assert from "node:assert/strict";
import test from "node:test";
import { assertCaptureTime, validateCaptureIntervals } from "../scripts/showcase-capture-checks.mjs";

const entry = { id: "capture", duration: 5, still_intervals: [[1, 2], [3, 4], [4, 5]] };
const sample = (requestedTime, publishedTime) => ({ requestedTime, publishedTime });

test("continuous motion cannot pass with an earlier frame", () => {
  validateCaptureIntervals(entry);
  assertCaptureTime(entry, sample(0.5, 0.5), 0.5);
  assertCaptureTime(entry, sample(2.5, 2.5 - 1e-8), 2.5);
  assert.throws(() => assertCaptureTime(entry, sample(2.5, 2), 2.5), /stale\/future/);
  assert.throws(() => assertCaptureTime(entry, sample(0.5, 0), 0.5), /stale\/future/);
});

test("only declared still intervals permit earlier publication", () => {
  assert.equal(assertCaptureTime(entry, sample(1.5, 1), 1.5), 1);
  assertCaptureTime(entry, sample(1.5, 1.5), 1.5);
  assert.throws(() => assertCaptureTime(entry, sample(1.5, 0.9), 1.5), /stale\/future/);
  assert.throws(() => assertCaptureTime(entry, sample(2.5, 1), 2.5), /stale\/future/);
});

test("discrete thresholds require the new state, not the preceding hold", () => {
  assertCaptureTime(entry, sample(3.9, 3), 3.9);
  assertCaptureTime(entry, sample(4, 4), 4);
  assert.throws(() => assertCaptureTime(entry, sample(4, 3), 4), /stale\/future/);
  assertCaptureTime(entry, sample(5, 4), 5);
});

test("malformed, future, negative, and mismatched timestamps fail", () => {
  for (const value of [null, NaN, Infinity, -1, "0.5", 0.6]) {
    assert.throws(() => assertCaptureTime(entry, sample(0.5, value), 0.5));
  }
  assert.throws(() => assertCaptureTime(entry, sample(0.6, 0.5), 0.5), /wrong sample/);
  for (const time of [-1, NaN, Infinity, 5.1]) {
    assert.throws(() => assertCaptureTime(entry, sample(time, time), time), /outside/);
  }
});

test("still intervals are explicit, finite, ordered, and bounded", () => {
  validateCaptureIntervals({ ...entry, still_intervals: [] });
  for (const still_intervals of [undefined, null, [[2, 1]], [[0, 6]], [[1, 3], [2, 4]],
    [[-1, 1]], [[NaN, 1]], [[1, Infinity]], [[1, 2, 3]], [[1, 1]], [["1", 2]]]) {
    assert.throws(() => validateCaptureIntervals({ ...entry, still_intervals }));
  }
});

import { assertCompletedCapture, captureSchedule, COMPLETION_PROBE_SLOP } from "../scripts/showcase-capture-checks.mjs";

test("only the final sample is a bounded source-completion probe", () => {
  const scene = { ...entry, thumbnail_time: 5, beats: [{ time: 1 }, { time: 3 }, { time: 5 }] };
  const schedule = captureSchedule(scene);
  assert.equal(schedule.completionTime, 5 + COMPLETION_PROBE_SLOP);
  assert.equal(schedule.posterTime, schedule.completionTime);
  assert.equal(schedule.frameTimes.at(-1), schedule.completionTime);
  assert.equal(schedule.frameTimes.includes(5), false);
  assert.deepEqual(schedule.wanted, [1, 3, schedule.completionTime]);
  assert.ok(schedule.frameTimes.slice(0, -1).every(time => time < 5));
  assert.equal(captureSchedule({ ...scene, thumbnail_time: 3 }).posterTime, 3);
});

test("completion probes reject missing completion and substantive endpoint mismatch", () => {
  const requestedTime = 5 + COMPLETION_PROBE_SLOP;
  for (const actual of [5 - Number.EPSILON * 4, 5 + Number.EPSILON * 4]) {
    const result = { requestedTime, publishedTime: actual, authoredDuration: actual, sourceCompleted: true, sourceState: "completed" };
    assertCompletedCapture(entry, result, requestedTime);
    for (const patch of [{ authoredDuration: null }, { sourceCompleted: false }, { sourceState: "running" },
      { authoredDuration: 4 }, { publishedTime: 4 }, { requestedTime: 5 }]) {
      assert.throws(() => assertCompletedCapture(entry, { ...result, ...patch }, requestedTime));
    }
  }
});
