import assert from 'node:assert/strict';
import test from 'node:test';
import { endpointAdmission } from './endpoint-admission.mjs';

const clock=1792112615240.6;
const sample={sawRun:true,sawProgress:true,epoch:[1,clock],progressRebuilds:0,
  progressFrames:311,
  observed:{runInFlight:false,patchState:'applied',backend:'WebGL2',errors:[],metrics:{
    time:25,frames:313,session:2,clockOriginMs:clock,ready:true,needsPresent:false,
    bufferedDeltas:0,counters:{rendererRebuilds:1,objectCount:10432,drawCalls:10430}}}};
const clone=()=>structuredClone(sample);
test('accept actual completion session handoff with coherent clock and rebuild',()=>{
 const result=endpointAdmission(clone());
 assert.equal(result.type,'completion-session-handoff');
 assert.equal(result.authoredEndSeconds,25);
 assert.equal(result.fromSession,1);
 assert.equal(result.toSession,2);
});
test('accept normal same renderer epoch completion',()=>{
 const a=clone();a.observed.metrics.session=1;a.observed.metrics.counters.rendererRebuilds=0;
 assert.equal(endpointAdmission(a).type,'same-renderer-session');
});
test('reject stale, wrong renderer epoch or unsignalled publication',()=>{
 const changes=[
  a=>{a.sawRun=false}, a=>{a.sawProgress=false},
  a=>{a.observed.patchState='error'}, a=>{a.observed.runInFlight=true},
  a=>{a.observed.metrics.ready=false}, a=>{a.observed.metrics.needsPresent=true},
  a=>{a.observed.metrics.bufferedDeltas=1},
  a=>{a.observed.metrics.time=24}, a=>{a.observed.metrics.time=25.3},
  a=>{a.observed.metrics.frames=311},
  a=>{a.observed.metrics.clockOriginMs=clock+1},
  a=>{a.observed.metrics.session=3},
  a=>{a.observed.metrics.counters.rendererRebuilds=0},
  a=>{a.observed.metrics.counters.rendererRebuilds=2},
  a=>{a.observed.metrics.counters.objectCount=10},
  a=>{a.observed.metrics.counters.drawCalls=0},
  a=>{a.observed.backend='WebGPU'},
  a=>{a.observed.errors=['fail']},
 ];
 for(const [index,mutate] of changes.entries()){
  const a=clone();mutate(a);
  assert.equal(endpointAdmission(a),null,`mutated case ${index}`);
 }
});
