import assert from 'node:assert/strict';
import test from 'node:test';
import { modelLookahead } from './preparation-lookahead.mjs';
const trace = (rows) => rows.map(([targetSec, completedMs]) => ({ targetSec, completedMs }));
test('readiness is required at interval start; end-based model incorrectly says zero', () => {
  const result = modelLookahead(trace([[1, 100], [2, 1600], [3, 2700]]));
  assert.equal(result.projectedBufferStallMs, 600);
  assert.deepEqual(result.segments.map(x => x.bufferStallMs), [0, 500, 100]);
  assert.deepEqual(result.segments.map(x => x.authoredStartSec), [0, 1, 2]);
});
test('a ready producer never causes buffering', () => {
  assert.equal(modelLookahead(trace([[1, 100], [2, 200], [3, 300]])).projectedBufferStallMs, 0);
});
test('single segment has initial readiness but no later buffering', () => {
  const result = modelLookahead(trace([[3, 1000]]), 32);
  assert.equal(result.initialReadyMs, 1000);
  assert.equal(result.projectedBufferStallMs, 0);
});
test('zero-duration barriers are ordered at the same authored time', () => {
  assert.equal(modelLookahead(trace([[0, 100], [0, 250], [1, 300]])).projectedBufferStallMs, 200);
});
test('speed affects consumption, not the measured producer trace', () => {
  const input = trace([[1, 100], [2, 1600], [3, 2700]]);
  const original = JSON.stringify(input);
  assert.equal(modelLookahead(input, 2).projectedBufferStallMs, 1600);
  assert.equal(JSON.stringify(input), original);
});
test('reject invalid speeds, values and reversed producer/time order', () => {
  for (const speed of [0, -1, NaN, Infinity]) assert.equal(modelLookahead(trace([[1, 0]]), speed).valid, false);
  for (const input of [[], null, [null], trace([[1, -1]]), trace([[-1, 0]]),
    trace([[2, 0], [1, 1]]), trace([[1, 1], [2, 0]]), trace([[Infinity, 0]])]) {
    assert.equal(modelLookahead(input).valid, false);
  }
});
test('nonrepresentable deadlines fail rather than becoming a favorable zero', () => {
  assert.equal(modelLookahead(trace([[1e308, 0], [1e308, 1]]), 1e-308).valid, false);
});
