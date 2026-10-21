import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import test from 'node:test';
const fixture=JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/family-iteration.json',import.meta.url),'utf8'));
test('iterator fixture records the exact pinned generator and method provenance',()=>{
 const generator=readFileSync(new URL('./manim-family-iteration-reference.py',import.meta.url));
 assert.equal(fixture.generatorSha256,createHash('sha256').update(generator).digest('hex'));
 assert.equal(fixture.schemaVersion,1);assert.equal(fixture.manimVersion,'0.21.0');
 assert.deepEqual(Object.keys(fixture.methodSourceSha256).sort(),['__iter__','__len__','split','add','add_to_back','remove','insert','invert'].sort());
 for(const hash of Object.values(fixture.methodSourceSha256))assert.match(hash,/^[a-f0-9]{64}$/);
});
test('every iterator capture and mutation control remains covered',()=>{
 const expected=new Set();
 for(const kind of ['Group','VGroup'])for(const ownPoints of [0,1,4,5])for(const consumed of [0,1,2,3])for(const action of ['none','remove_first','clear_children','append_extra','add_empty','prepend_extra','insert_extra','reverse','set_points','clear_points'])expected.add(JSON.stringify({class:kind,ownPoints,consumed,action}));
 assert.equal(fixture.cases.length,320);assert.deepEqual(new Set(fixture.cases.map(row=>JSON.stringify(row.spec))),expected);
 assert.ok(fixture.cases.every(row=>row.staysExhausted));
});
