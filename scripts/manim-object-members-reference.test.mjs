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

test('membership/copy Cairo source preserves the complete public Python example body',()=>{
 const source=readFileSync(new URL('../web/python/examples/manim_parity_object_members.py',import.meta.url),'utf8');
 const expected=source.replace('from noon import *','from manim import *')
   .replace('async def construct(self):','def construct(self):').replaceAll('await self.wait(','self.wait(');
 const canonical=readFileSync(new URL('../parity/manim-v0.21/core-examples/object_membership_copy.py',import.meta.url),'utf8');
 assert.equal(canonical,expected,'only host import and async wait syntax may differ');
});
