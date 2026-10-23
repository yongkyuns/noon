import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import test from 'node:test';

const fixture=JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/xyz-queries.json',import.meta.url),'utf8'));
const sha=path=>createHash('sha256').update(readFileSync(new URL(path,import.meta.url))).digest('hex');

test('XYZ query controls pin the actual reference generator and observers',()=>{
 assert.equal(fixture.schemaVersion,1);
 assert.equal(fixture.manimVersion,'0.21.0');
 assert.equal(fixture.generatorSha256,sha('./manim-xyz-queries-reference.py'));
 assert.equal(fixture.observerSha256,sha('./manim_xyz_query_observation.py'));
 assert.equal(fixture.helperSha256,sha('./manim_depth_affine_observation.py'));
 assert.equal(Object.keys(fixture.methodSourceSha256).length,25);
 for(const digest of Object.values(fixture.methodSourceSha256))assert.match(digest,/^[a-f0-9]{64}$/);
});

test('all40XYZprofiles retain four lifecycle phases and all38 original observations',()=>{
 const expected=new Set();
 for(const kind of ['Mobject','VMobject','Group','VGroup'])
  for(const rows of [0,1,4,5,8])for(const angle of [0,.37])expected.add(JSON.stringify({kind,rows,angle}));
 assert.equal(fixture.cases.length,40);
 assert.deepEqual(new Set(fixture.cases.map(row=>JSON.stringify(row.spec))),expected);
 assert.deepEqual(fixture.phases,['authored','bound','edited','removed']);
 for(const row of fixture.cases){
  assert.deepEqual(row.phases.map(phase=>phase.phase),fixture.phases);
  for(const phase of row.phases){
   assert.deepEqual(phase.value.spec,row.spec);
   assert.equal(phase.value.observations.length,38);
   assert.equal(phase.value.observations.filter(query=>query.method==='reduce_across_dimension').length,9);
  }
 }
});

const phase=(kind,rows,name='authored')=>fixture.cases.find(row=>row.spec.kind===kind&&row.spec.rows===rows&&row.spec.angle===0).phases.find(row=>row.phase===name).value.observations;
const depthMaximum=(kind,name)=>phase(kind,4,name).find(query=>query.method==='get_extremum_along_dim'&&query.keywords.dim===2&&query.keywords.key===1).output;

test('controls distinguish raw versus vector depth, empty shapes and exact tail errors',()=>{
 assert.equal(depthMaximum('Mobject','authored').result.value,43.375);
 assert.equal(depthMaximum('VMobject','authored').result.value,10.5);
 assert.equal(depthMaximum('Mobject','edited').result.value,44.5);
 assert.equal(depthMaximum('VMobject','edited').result.value,11.625);
 assert.deepEqual(depthMaximum('VMobject','removed'),depthMaximum('VMobject','edited'));
 const boundary=(kind,rows)=>phase(kind,rows).find(query=>query.method==='get_points_defining_boundary').output;
 assert.deepEqual(boundary('Mobject',0).result.shape,[0,3]);
 assert.deepEqual(boundary('VMobject',0).result.shape,[0]);
 assert.deepEqual(boundary('VMobject',5),{kind:'raised',type:'ValueError',message:'zip() argument 2 is shorter than argument 1'});
});
