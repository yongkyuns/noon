import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import test from 'node:test';

const fixture=JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/scalar-xyz.json',import.meta.url)));
const original=JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/scalar-path-precision.json',import.meta.url)));
test('scalar XYZ observations pin CE source, deterministic profiles, and all three axes',()=>{
  assert.equal(fixture.manimVersion,'0.21.0');
  for(const [field,file] of [['generatorSha256','./manim-scalar-xyz-reference.py'],['observerSha256','./manim_scalar_xyz_observation.py']]) {
    assert.equal(fixture[field],createHash('sha256').update(readFileSync(new URL(file,import.meta.url))).digest('hex'));
  }
  assert.equal(fixture.cases.length,225);
  assert.equal(Object.keys(fixture.methodSourceSha256).length,8);
  for(const [method,hash] of Object.entries(fixture.methodSourceSha256)) {
    assert.match(hash,/^[a-f0-9]{64}$/);
    if(original.methodSourceSha256[method])assert.equal(hash,original.methodSourceSha256[method]);
  }
  assert.ok(fixture.cases.every(row=>row.inputUnchanged && row.pointsXYZ.every(point=>point.length===3)));
  assert.equal(fixture.cases.filter(row=>row.outcome.kind==='raised').length,4);
  const coincident=fixture.cases.find(row=>row.spec.coincidentXY);
  assert.equal(coincident.outcome.kind,'returned');
  assert.equal(coincident.pointsXYZ.length,8);
  assert.deepEqual(coincident.pointsXYZ.at(-1),coincident.pointsXYZ[0]);
  assert.notEqual(coincident.beforeXYZ.at(-1)[2],coincident.beforeXYZ[0][2]);
  assert.ok(fixture.cases.filter(row=>['set_points_as_corners','add_points_as_corners'].includes(row.spec.method)).every(row=>row.spec.stride===4));
});
