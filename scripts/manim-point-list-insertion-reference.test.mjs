import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const read = file => readFileSync(new URL(file, import.meta.url), 'utf8');
const hash = value => createHash('sha256').update(value).digest('hex');
const fixture = JSON.parse(read('../fixtures/manim-v0.21/point-list-insertion.json'));
const oracle = JSON.parse(read('../fixtures/manim-v0.21/point-list-insertion-oracle.json'));

test('pure point-list controls retain the original 135 CE outcomes and generator', () => {
  assert.equal(fixture.manimVersion, '0.21.0');
  assert.equal(fixture.schemaVersion, 1);
  assert.equal(fixture.cases.length, 135);
  assert.equal(fixture.probeSha256, hash(read('./manim-point-list-insertion-reference.py')));
  assert.equal(fixture.methodSourceSha256, hash(fixture.methodSource));
  assert.equal(fixture.methodSourceSha256, '68c645e4fa21730b2cea7d418a0c2eba7b59e1f23f66679fb4c76495c9a9732b');
  assert.equal(fixture.cases.filter(row => row.output.kind === 'returned').length, 89);
  assert.equal(fixture.cases.filter(row => row.output.kind === 'raised').length, 46);
  assert.ok(fixture.cases.every(row => row.inputUnchanged && row.ownPointsUnchanged));
});

test('same-platform oracle contains exactly the six pinned pure CE functions', () => {
  assert.equal(oracle.manimVersion, '0.21.0');
  assert.equal(oracle.rawMethodSha256, fixture.methodSourceSha256);
  assert.equal(oracle.generatorSha256, hash(read('./manim-point-list-platform-reference.py')));
  assert.match(oracle.licenses.LICENSE, /Copyright \(c\) 2018 3Blue1Brown LLC/);
  assert.match(oracle.licenses['LICENSE.community'], /Copyright \(c\) 2024, the Manim Community Developers/);
  assert.deepEqual(Object.keys(oracle.sources), [
    'insert_n_curves_to_point_list', 'get_cubic_bezier_tuples_from_points',
    'gen_cubic_bezier_tuples_from_points', 'bezier_remap', 'subdivide_bezier',
    '_get_subdivision_matrix',
  ]);
  for (const [name, source] of Object.entries(oracle.sources)) {
    assert.equal(hash(source), oracle.sha256[name], name);
  }
  assert.equal(oracle.sources.insert_n_curves_to_point_list,
    fixture.methodSource.replace(/^    /gm, ''));
  assert.deepEqual(oracle.subdivisionTables, [{}, {}, {}, {}]);
});
