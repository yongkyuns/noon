import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import test from 'node:test';
const fixture=JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/object-queries.json',import.meta.url),'utf8'));
test('ordinary-object controls have pinned source and complete class/point coverage',()=>{
 const source=readFileSync(new URL('./manim-object-queries-reference.py',import.meta.url));
 assert.equal(fixture.generatorSha256,createHash('sha256').update(source).digest('hex'));assert.equal(fixture.manimVersion,'0.21.0');assert.equal(fixture.schemaVersion,1);assert.equal(fixture.cases.length,1020);
 assert.deepEqual(Object.keys(fixture.methodSourceSha256).sort(),['__getitem__','__iter__','__len__','get_group_class','split','get_array_attrs','apply_over_attr_arrays'].sort());
 const specs=new Set(fixture.cases.map(row=>JSON.stringify(row.spec)));assert.equal(specs.size,1020);
 for(const kind of ['Mobject','VMobject','Circle','Square','VectorizedPoint'])for(const ownPoints of [0,1,4,5])for(const action of ['none','set_points','clear_points']) {
  for(const index of [-2,-1,0,1,10])assert.ok(specs.has(JSON.stringify({class:kind,ownPoints,query:'scalar',phase:'index',index,action})));
  for(const slice of [[null,null,1],[null,null,-1],[0,1,1],[1,3,1],[0,0,1],[-1,null,-1]])for(const phase of ['start','step'])assert.ok(specs.has(JSON.stringify({class:kind,ownPoints,query:'slice',phase,slice,action})));
 }
});
