import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import test from 'node:test';
const fixture=JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/object-members.json',import.meta.url),'utf8'));
test('ordinary-object membership controls pin source, complete cases and retained prepend semantics',()=>{
 const source=readFileSync(new URL('./manim-object-members-reference.py',import.meta.url));
 assert.equal(fixture.generatorSha256,createHash('sha256').update(source).digest('hex'));
 assert.equal(fixture.manimVersion,'0.21.0');assert.equal(fixture.schemaVersion,1);assert.equal(fixture.cases.length,50);
 const keys=new Set(fixture.cases.map(row=>JSON.stringify(row.spec)));assert.equal(keys.size,50);
 for(const kind of ['Mobject','VMobject','Circle','Square','VectorizedPoint'])for(const ownPoints of [0,4])for(const index of [-100,-1,0,1,100])assert.ok(keys.has(JSON.stringify({class:kind,ownPoints,index})));
 assert.deepEqual(Object.keys(fixture.methodSourceSha256).sort(),['add','remove','add_to_back','insert','copy','_assert_valid_submobjects_internal'].sort());
 for(const row of fixture.cases){assert.deepEqual(row.steps[3].capturedIterator,row.spec.ownPoints?['self']:[]);assert.equal(row.copy.aliasesPreserved,true);assert.equal(row.copy.sourceUnchanged,true);}
});
