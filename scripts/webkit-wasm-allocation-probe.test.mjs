import assert from "node:assert/strict";
import test from "node:test";
import {repeatedProbeQueue, workerProbeSource} from "./webkit-wasm-allocation-probe.mjs";
const entry={id:"showcase-text-math"};
const queue=[{entry,noJspi:false},{entry,noJspi:true}];
test("counterbalance exact-source JSPI and no-JSPI trials",()=>{
  const cases=repeatedProbeQueue(queue,4);
  assert.deepEqual(cases.map(x=>[x.probeTrial,x.noJspi]),[
    [1,false],[1,true],[2,true],[2,false],
    [3,false],[3,true],[4,true],[4,false]]);
  assert.ok(cases.every(x=>x.entry===entry));
});
test("reject incomplete or unbalanced comparisons",()=>{
  for(const n of [0,25,1.5,NaN])assert.throws(()=>repeatedProbeQueue(queue,n));
  assert.throws(()=>repeatedProbeQueue(queue.slice(0,1),2));
});
test("worker telemetry is source-independent and preserves both execution modes",()=>{
  for(const mode of [false,true]){
    const text=workerProbeSource(mode);
    assert.match(text,/await import\("\.\/python-worker\.js"\)/);
    assert.match(text,/noon_allocator_probe_snapshot/);
    assert.match(text,/WebAssembly\.instantiateStreaming/);
    assert.match(text,/memory\.buffer\.byteLength/);
    assert.equal(text.includes("if (true) {"),mode);
    assert.doesNotThrow(()=>new Function("return (async()=>{" + text + "})()"));
  }
});
