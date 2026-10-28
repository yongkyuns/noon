// Independent literal source/alpha witnesses plus full-image host equality.
// Native sampled-image tests separately qualify every texel, including edges.
import assert from "node:assert/strict";
import {qualifyPairedAuthoring} from "./paired-authoring-qualification.mjs";
import {disableAuthoringJspi} from "./playground-browser-support.mjs";
function reference(image,time) {
  assert.equal(image.width,640);assert.equal(image.height,360);
  const declaredSize=time<0.5?12:time<1.5?12-11*(time-0.5):time<2||time>=3?1:8;
  const size=Math.max(1,Math.round(declaredSize*256)/256);
  let maximumByteError=0,channels=0;
  for(let y=148;y<212;y++)for(let x=350;x<450;x++) {
    const p=[x+0.5,y+0.5],origin=[368,164];
    const sample=size<=1?p:p.map((v,a)=>Math.floor(origin[a]+(Math.floor((v-origin[a])/size)+0.5)*size)+0.5);
    if([368,384,416,432].some(edge=>Math.abs(sample[0]-edge)<1)||[164,196].some(edge=>Math.abs(sample[1]-edge)<1))continue;
    const red=sample[0]>=368&&sample[0]<416&&sample[1]>=164&&sample[1]<196;
    const blue=sample[0]>=384&&sample[0]<432&&sample[1]>=164&&sample[1]<196;
    let premul=red?[0.6,0,0,0.6]:[0,0,0,0];
    if(blue)premul=premul.map((v,c)=>v*0.5+(c===2||c===3?0.5:0));
    const source=premul.map(v=>Math.round(v*255)/255*0.75);
    for(let c=0;c<4;c++) {
      const expected=Math.round(255*source[c]+[24,48,64,255][c]*(1-source[3]));
      const error=Math.abs(image.data[(y*640+x)*4+c]-expected);
      maximumByteError=Math.max(maximumByteError,error);channels++;
      assert.ok(error<=2,`Pixelate t=${time} cell=${size} at=${x},${y} c=${c}: ${error}`);
    }
  }
  if(size<=1)for(let y=148;y<212;y++)for(let x=350;x<450;x++) {
    const at=(y*640+x)*4,reference=(y*640+x-160)*4;
    assert.deepEqual(image.data.slice(at,at+4),image.data.slice(reference,reference+4),"neutral/removal must be exact ordinary composition");
  }
  return {size,channels,maximumByteError,identityExact:size<=1};
}
const times=[0.25,0.5,0.6,0.625,0.75,0.9,1,1.1,1.25,1.375,1.4,1.5,1.75,2,2.25,2.5,2.75,3,3.25,3.5];
await qualifyPairedAuthoring({
  cases:times.map(sampleTime=>({id:`pixelate-${sampleTime}`,file:"pixelate.py",factory:"createDirectPixelateSmokeRenderer",duration:3.5,sampleTime,playback:"live",canvasSize:[640,360],
    directHeldSampleTime:sampleTime===3.5||(sampleTime>=0.5&&sampleTime<1.5)?undefined:Math.floor(sampleTime*2)/2,
    boundaries:[0.5,1.5,2,2.5,3],stopAtSourceCompletion:sampleTime===3.5})),
  artifactDirectory:"browser-smoke-artifacts/pixelate",prepareContext:disableAuthoringJspi,
  qualifyPixels({fixture,rust,python}) {return {rust:reference(rust,fixture.sampleTime),python:reference(python,fixture.sampleTime)};},
});
