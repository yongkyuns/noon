import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = fileURLToPath(new URL('../../', import.meta.url));

test('M0 visual-effects independent reference and negative controls', () => {
  const result = spawnSync('python3', ['tests/visual-effects/test_reference.py', '-v'], {
    cwd: root,
    encoding: 'utf8',
    timeout: 30_000,
    maxBuffer: 1024 * 1024,
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  const count = /Ran (\d+) tests? in/.exec(result.stderr);
  assert.ok(count && Number(count[1]) >= 24, 'reference discovery must not pass with missing tests');
});
