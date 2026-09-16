import assert from 'node:assert/strict';
import test from 'node:test';
import { IMAGE_SAMPLE_TIMES, validateDirectImageCapture } from '../scripts/image-raster-contract.mjs';

function capture(time, backend = 'WebGPU') {
  const waiting = time >= 3;
  return {
    requestedTime: time, publishedTime: waiting ? 3 : time, backend,
    count: time < 1 || waiting ? 2 : 3,
    wake: { presentNow: false, cadence: waiting ? 'timer' : 'animation-frame',
      delayMs: waiting ? (4 - time) * 1000 : null },
  };
}

test('all image phases retain exact active samples and a deadline-only final wait', () => {
  for (const time of IMAGE_SAMPLE_TIMES) {
    validateDirectImageCapture(capture(time), time, 'webgpu');
    validateDirectImageCapture(capture(time, 'WebGL2'), time, 'webgl');
  }
});

test('idle wait cannot hide shifted active samples or mismatched backend and membership', () => {
  for (const change of [
    { publishedTime: 1 }, { requestedTime: 1 }, { backend: 'WebGL2' }, { count: 2 },
    { wake: { presentNow: true, cadence: 'animation-frame' } },
  ]) {
    assert.throws(() => validateDirectImageCapture({ ...capture(1.5), ...change }, 1.5, 'webgpu'));
  }
});

test('idle capture proves no intermediate publication, polling, or deadline drift', () => {
  const idle = capture(3.5);
  for (const change of [
    { publishedTime: 3.5 }, { publishedTime: 2.5 },
    { wake: { ...idle.wake, cadence: 'animation-frame' } },
    { wake: { ...idle.wake, delayMs: 1000 } },
    { wake: { ...idle.wake, presentNow: true } },
  ]) {
    assert.throws(() => validateDirectImageCapture({ ...idle, ...change }, 3.5, 'webgpu'));
  }
});
