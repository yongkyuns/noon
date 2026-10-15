// Fast address-model/source contract only; not a browser or raster qualification.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

test('CUDA transpose lesson: addressing, broadcasts, padding, and source contract', () => {
  const testFile = fileURLToPath(new URL('../tests/cuda_transpose_banks_test.py', import.meta.url));
  const result = spawnSync('python3', [testFile], { encoding: 'utf8', timeout: 30_000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
});
