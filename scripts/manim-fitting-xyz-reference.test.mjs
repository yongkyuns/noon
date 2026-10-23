import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const fixture = JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/fitting-xyz.json', import.meta.url)));
test('XYZ fitting controls pin CE extents, pivots, unfinished arrays and aliases', () => {
  assert.equal(fixture.manimVersion, '0.21.0');
  for (const [field, file] of [['generatorSha256', './manim-fitting-xyz-reference.py'], ['observerSha256', './manim_fitting_xyz_observation.py'], ['helperSha256', './manim_depth_affine_observation.py']]) {
    assert.equal(fixture[field], createHash('sha256').update(readFileSync(new URL(file, import.meta.url))).digest('hex'));
  }
  assert.equal(fixture.cases.length, 720);
  assert.equal(new Set(fixture.cases.map(row => JSON.stringify(row.spec))).size, 720);
  assert.equal(fixture.cases.filter(row => row.outcome.kind === 'raised').length, 108);
  assert.ok(fixture.cases.filter(row => row.outcome.kind === 'raised').every(row => row.spec.rows === 5 && row.spec.pivot !== 'point'));
  assert.deepEqual(new Set(fixture.cases.map(row => row.spec.dimension)), new Set([0, 1, 2]));
  assert.deepEqual(new Set(fixture.cases.map(row => row.spec.length)), new Set([0, 7, -7]));
  for (const hash of Object.values(fixture.methodSourceSha256)) assert.match(hash, /^[a-f0-9]{64}$/);
  assert.ok(fixture.cases.some(row => row.before.depth > 0 && row.spec.dimension === 0 && !row.spec.stretch && row.after.depth !== row.before.depth));
});
