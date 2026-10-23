import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const fixture = JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/scale-xyz.json', import.meta.url)));
test('generic XYZ scaling pins CE scalar/vector factors, aliases, pivots and unfinished paths', () => {
  assert.equal(fixture.manimVersion, '0.21.0');
  for (const [field, file] of [['generatorSha256', './manim-scale-xyz-reference.py'], ['observerSha256', './manim_scale_xyz_observation.py'], ['helperSha256', './manim_depth_affine_observation.py']]) {
    assert.equal(fixture[field], createHash('sha256').update(readFileSync(new URL(file, import.meta.url))).digest('hex'));
  }
  assert.equal(fixture.cases.length, 288);
  assert.equal(new Set(fixture.cases.map(row => JSON.stringify(row.spec))).size, 288);
  assert.equal(fixture.cases.filter(row => row.outcome.kind === 'returned' && row.outcome.returnsSelf).length, 256);
  const rejected = fixture.cases.filter(row => row.outcome.kind === 'raised');
  assert.equal(rejected.length, 32);
  assert.ok(rejected.every(row => row.spec.shape === 'path5' && row.spec.pivot !== 'point'
    && row.outcome.type === 'ValueError' && row.outcome.message === 'zip() argument 2 is shorter than argument 1'));
  for (const hash of Object.values(fixture.methodSourceSha256)) assert.match(hash, /^[a-f0-9]{64}$/);
});
