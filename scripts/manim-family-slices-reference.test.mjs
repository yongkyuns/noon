import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import test from 'node:test';
const fixture=JSON.parse(readFileSync(new URL('../fixtures/manim-v0.21/family-slices.json',import.meta.url),'utf8'));
test('slice fixture preserves the pinned generator and protocol method provenance',()=>{
 const generator=readFileSync(new URL('./manim-family-slices-reference.py',import.meta.url));
 assert.equal(fixture.generatorSha256,createHash('sha256').update(generator).digest('hex'));
 assert.equal(fixture.schemaVersion,1);assert.equal(fixture.manimVersion,'0.21.0');
 assert.deepEqual(Object.keys(fixture.methodSourceSha256).sort(),['__getitem__','__len__','get_group_class','has_points','has_no_points','split','add','remove'].sort());
 assert.deepEqual(fixture.semanticProjection,['spec','before','events','result','after']);
 for(const hash of Object.values(fixture.methodSourceSha256))assert.match(hash,/^[a-f0-9]{64}$/);
});
test('all own-root, slice, callback-phase and edit controls remain covered',()=>{
 const expected=new Set();
 for(const kind of ['Group','VGroup'])for(const ownPoints of [0,4])for(const slice of [[0,3,1],[1,3,1],[-1,null,-1],[null,null,1],[null,null,-1],[0,0,1]])for(const phase of ['start','stop','step'])for(const action of ['none','remove_first','clear_children','append_extra','set_points','clear_points'])expected.add(JSON.stringify({class:kind,ownPoints,slice,phase,action}));
 assert.equal(fixture.cases.length,432);assert.deepEqual(new Set(fixture.cases.map(row=>JSON.stringify(row.spec))),expected);
 assert.ok(fixture.cases.some(row=>row.result.exception==='IndexError'));
});
