import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import test from 'node:test';
const fixture=JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/scalar-path-precision.json',import.meta.url)));
test('scalar precision controls pin the shared observer, CE methods and generator',()=>{
 assert.equal(fixture.manimVersion,'0.21.0');
 assert.equal(fixture.mutableStrideClosingObserverSha256,createHash('sha256').update(readFileSync(new URL('./manim_mutable_stride_closing_observation.py',import.meta.url))).digest('hex'));
 assert.deepEqual(fixture.mutableStrideClosingCases.map(row=>row.pointCount),[10,11]);
 assert.ok(fixture.mutableStrideClosingCases.every(row=>row.stride===5&&row.returnsSelf));
 for(const [field,file] of [['generatorSha256','./manim-scalar-path-precision-reference.py'],['observerSha256','./manim_scalar_path_observation.py']]){
  assert.equal(fixture[field],createHash('sha256').update(readFileSync(new URL(file,import.meta.url))).digest('hex'));
 }
 const names=['set_points','append_points','start_new_path','add_line_to','add_quadratic_bezier_curve_to','add_cubic_bezier_curve_to','add_cubic_bezier_curve','add_points_as_corners','set_points_as_corners','close_path'];
 assert.deepEqual(fixture.cases.map(row=>row.method),names);
 assert.deepEqual(Object.keys(fixture.methodSourceSha256),names);
 assert.ok(fixture.cases.every(row=>row.returnsSelf));
 for(const hash of Object.values(fixture.methodSourceSha256))assert.match(hash,/^[a-f0-9]{64}$/);
});
