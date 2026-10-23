import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const fixture = JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/rotated-xyz.json', import.meta.url)));
test('rotated XYZ controls pin CE and retain all matrix and command-path profiles', () => {
  assert.equal(fixture.manimVersion, '0.21.0');
  for (const [field, file] of [['generatorSha256', './manim-rotated-xyz-reference.py'], ['observerSha256', './manim_rotated_xyz_observation.py'], ['helperSha256', './manim_depth_affine_observation.py']]) {
    assert.equal(fixture[field], createHash('sha256').update(readFileSync(new URL(file, import.meta.url))).digest('hex'));
  }
  assert.equal(fixture.cases.length, 648);
  assert.equal(new Set(fixture.cases.map(row => JSON.stringify(row.spec))).size, 648);
  assert.ok(fixture.cases.every(row => row.outcome.kind === 'returned' && row.outcome.returnsSelf));
  assert.deepEqual(new Set(fixture.cases.map(row => row.spec.shape)), new Set(['path4', 'path8', 'line']));
  assert.equal(fixture.cases.filter(row => row.spec.angle === .37 && row.spec.stretch && row.spec.dimension < 2).length, 108);
  for (const hash of Object.values(fixture.methodSourceSha256)) assert.match(hash, /^[a-f0-9]{64}$/);
  const planar = fixture.planarCommandControl;
  assert.ok(planar.pointsXYZ.every(row => row.length === 3 && row[2] === 0));
  assert.notEqual(planar.shift[0], Math.fround(planar.shift[0]));
  assert.notEqual(planar.pivot[0], Math.fround(planar.pivot[0]));
});
