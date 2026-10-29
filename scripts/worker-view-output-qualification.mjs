// Real Pyodide -> execution worker -> render worker, paired with direct Rust/WASM.
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {fileURLToPath} from "node:url";
import {qualifyPairedAuthoring} from "./paired-authoring-qualification.mjs";
import {disableAuthoringJspi} from "./playground-browser-support.mjs";
import {createRuntimeBuildIdentity} from "./build-runtime-identity.mjs";
const root=fileURLToPath(new URL("..",import.meta.url));
const runtime=JSON.parse(await readFile(root+"/web/runtime-build-identity.json","utf8"));
assert.deepEqual(await createRuntimeBuildIdentity(root),runtime,"package must match exact clean source");
assert.ok(runtime.sourceRevision);
const decode=x=>x<=0.04045?x/12.92:((x+0.055)/1.055)**2.4;
const encode=x=>x<=0.0031308?12.92*x:1.055*x**(1/2.4)-0.055;
function reference(image,time){
 const phase=time<=0.5?0:time<2.5?(time-0.5)/2:time<=3?1:time<5?(5-time)/2:0;
 const rgb=[0.2,0.4,0.8].map(x=>decode(x)*(1+7*phase));
 const l=rgb.reduce((v,x,i)=>v+x*[0.2126,0.7152,0.0722][i],0);
 // The center's complete Gaussian footprint lies inside the uniform circle.
 const expected=rgb.map(x=>{const y=x*(1+3*phase*Math.max(0,l-0.15)/l);return Math.round(255*encode(y/(1+y)));});
 const at=(Math.floor(image.height/2)*image.width+Math.floor(image.width/2))*4;
 const error=Math.max(...expected.map((v,i)=>Math.abs(v-image.data[at+i])));
 assert.ok(error<=2,`independent material/Bloom/transfer reference: ${error}`);
 assert.equal(image.data[at+3],255);
 return {maximumCenterError:error,expected,actual:[...image.data.subarray(at,at+4)],runtimeBuild:runtime.buildId};
}
await qualifyPairedAuthoring({
 cases:[0,0.5,1.5,2.5,2.75,3,4,5].map(sampleTime=>({
  id:`camera-bloom-${sampleTime}`,file:"camera_bloom.py",factory:"createDirectCameraBloomRenderer",
  duration:5,sampleTime,playback:"live",boundaries:[0.5,2.5,3],
  directHeldSampleTime:sampleTime===2.75?2.5:undefined,
  stopAtSourceCompletion:sampleTime===5,
 })),
 artifactDirectory:"ci-artifacts/worker-view-output",prepareContext:disableAuthoringJspi,
 qualifyPixels({fixture,rust,python}){return {rust:reference(rust,fixture.sampleTime),python:reference(python,fixture.sampleTime)};},
});
