import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const fixture = JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/depth-affine.json', import.meta.url)));
test('depth affine controls pin actual CE, full coordinates and family alias cases', () => {
  assert.equal(fixture.manimVersion, '0.21.0');
  for (const [field, file] of [['generatorSha256', './manim-depth-affine-reference.py'], ['observerSha256', './manim_depth_affine_observation.py']]) {
    assert.equal(fixture[field], createHash('sha256').update(readFileSync(new URL(file, import.meta.url))).digest('hex'));
  }
  assert.equal(fixture.cases.length, 75);
  assert.equal(new Set(fixture.cases.map(row => JSON.stringify(row.spec))).size, 75);
  assert.equal(Object.keys(fixture.methodSourceSha256).length, 4);
  for (const hash of Object.values(fixture.methodSourceSha256)) assert.match(hash, /^[a-f0-9]{64}$/);
  assert.ok(fixture.cases.every(row => row.returnsSelf && row.pointsXYZ.flat().every(point => point.length === 3)));
  assert.equal(fixture.cases.filter(row => row.spec.family).length, 15);
  assert.equal(fixture.cases.filter(row => row.spec.rows === 0).length, 15);
  assert.deepEqual(new Set(fixture.cases.map(row => row.spec.pivot)), new Set(['center', 'point', 'min', 'max', 'zero_edge']));
});
