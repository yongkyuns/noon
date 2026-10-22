import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const fixture = JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/path-keywords.json', import.meta.url)));
test('path keyword controls pin the shared observer, generator and all five CE methods', () => {
  for (const [field, file] of [['generatorSha256', './manim-path-keyword-reference.py'],
                              ['observerSourceSha256', './manim_path_keyword_observation.py']]) {
    assert.equal(fixture[field], createHash('sha256').update(readFileSync(new URL(file, import.meta.url))).digest('hex'));
  }
  assert.equal(fixture.schemaVersion, 1);
  assert.equal(fixture.manimVersion, '0.21.0');
  const methods = ['append_points', 'add_cubic_bezier_curve_to', 'add_quadratic_bezier_curve_to',
                   'append_vectorized_mobject', 'move_to'];
  assert.deepEqual(fixture.cases.map(row => row.method), methods);
  assert.deepEqual(Object.keys(fixture.methodSourceSha256), methods);
  assert.ok(fixture.cases.every(row => row.returnsSelf));
  assert.deepEqual(fixture.quadraticPrecisionCases.map(row => row.spec.id),
    ['fractional-singleton', 'fractional-complete-prefix']);
  assert.ok(fixture.quadraticPrecisionCases.every(row => row.returnsSelf));
  for (const row of fixture.quadraticPrecisionCases) {
    assert.deepEqual(row.pointsXY.slice(0, row.spec.prefix.length), row.spec.prefix.map(p => p.slice(0, 2)));
    assert.deepEqual(row.pointsXY.at(-1), row.spec.anchor.slice(0, 2));
  }
  assert.deepEqual(fixture.cases.map(row => row.keywords), [
    ['new_points'], ['handle1', 'handle2', 'anchor'], ['handle', 'anchor'],
    ['vectorized_mobject'], ['point_or_mobject'],
  ]);
});
