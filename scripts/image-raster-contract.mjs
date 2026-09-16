// Assertions for the shared raster_image live program, not animation evaluation.
import assert from 'node:assert/strict';

export const IMAGE_SAMPLE_TIMES = Object.freeze([0.5, 1, 1.5, 2, 2.5, 3, 3.5]);
const WAIT_START = 3;
const WAIT_END = 4;

export function validateDirectImageCapture(capture, requestedTime, backend) {
  assert.equal(capture.requestedTime, requestedTime, 'capture must identify the requested sample');
  assert.equal(capture.backend, backend === 'webgpu' ? 'WebGPU' : 'WebGL2');
  assert.equal(capture.count, requestedTime < 1 || requestedTime >= WAIT_START ? 2 : 3,
    'image add/FadeOut membership must match the shared program');
  assert.equal(capture.wake.presentNow, false, 'capture requires a settled publication');
  if (requestedTime >= WAIT_START && requestedTime < WAIT_END) {
    // wait(1) has no active visual channels. The ordinary realtime host schedules
    // its deadline instead of advancing/publishing at arbitrary intermediate times.
    assert.equal(capture.publishedTime, WAIT_START, 'idle wait must keep its endpoint frame');
    assert.equal(capture.wake.cadence, 'timer', 'idle wait must not poll animation frames');
    assert.equal(capture.wake.delayMs, (WAIT_END - requestedTime) * 1000,
      'idle wait must retain the exact runtime-authored deadline');
  } else {
    assert.equal(capture.publishedTime, requestedTime, 'active image animation must reach its sample');
    assert.equal(capture.wake.cadence, 'animation-frame');
  }
}
