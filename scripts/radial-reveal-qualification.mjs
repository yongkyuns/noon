// Real paired direct Rust/WASM and Python pixels, with an independent authored
// radial-field oracle and exact source/hidden endpoints on both browser backends.
import assert from 'node:assert/strict';
import {qualifyPairedAuthoring} from './paired-authoring-qualification.mjs';
import {disableAuthoringJspi} from './playground-browser-support.mjs';
import {radialSample,cleanRadial,burnRadial} from './radial-mask-reference.mjs';

function state(time,composed) {
  if(time<=1) return {progress:1-time,hole:false,stage:0,motion:0};
  if(time<1.25) return {progress:0,hole:false,stage:0,motion:0};
  if(time<=2.25) return {progress:time-1.25,hole:true,stage:1,motion:time-1.25};
  if(composed||time<2.5) return {progress:1,hole:true,stage:1,motion:1};
  if(time<=3.5) return {progress:3.5-time,hole:false,stage:2,motion:0};
  return {progress:0,hole:false,stage:2,motion:0};
}
function reference(image,fixture) {
  const composed=fixture.composed===true,p=state(fixture.sampleTime,composed);
  const scale=image.height/8;
  let checks=0,maximumError=0,holeChecks=0;
  function check(world,source,bounds,style) {
    const x=Math.floor(image.width/2+world[0]*scale),y=Math.floor(image.height/2-world[1]*scale);
    const point=[(x+.5-image.width/2)/scale,(image.height/2-y-.5)/scale];
    const uv=point.map((v,i)=>(v-bounds[i])/[bounds[2],bounds[3]][i]);
    const expected=radialSample(source,uv,p.progress,{...style,hole:p.hole},[bounds[2]*scale,bounds[3]*scale]);
    const at=(y*image.width+x)*4;
    for(let c=0;c<3;c++) {
      const error=Math.abs(Math.round(expected[c]*255)-image.data[at+c]);
      maximumError=Math.max(maximumError,error);
      assert.ok(error<=2,`${fixture.id} (${x},${y}) channel${c}: ${image.data[at+c]} != ${expected[c]*255}`);
    }
    checks++;
  }
  if(!composed) {
    const style=p.stage===0?cleanRadial:{...burnRadial,seed:7};
    for(let y=-.8;y<=.8;y+=.2) for(let x=-1.8;x<=1.8;x+=.2)
      check([x,y],[.2,.4,.9,1],[-2,-1,4,2],style);
  } else {
    const dy=p.stage===1?.5*p.motion:0,opacity=p.stage===1?1-.4*p.motion:1;
    // Pixel-center samples safely inside each nearest image texel, including
    // hidden magenta and partially transparent corners.
    for(let j=0;j<4;j++) for(let i=0;i<4;i++) {
      const center=i>=1&&i<=2&&j>=1&&j<=2;
      const corner=(i===0||i===3)&&(j===0||j===3);
      const alpha=(center?0:corner?128/255:1)*opacity;
      if(center)holeChecks++;
      check([-2.6+i*.4,-.6+j*.4+dy],[.2*alpha,.6*alpha,alpha,alpha],[-2.8,-.8+dy,1.6,1.6],
        p.stage===0?cleanRadial:{...burnRadial,seed:7});
    }
    // The overlap is composited once before one group mask and one group fade.
    for(const x of [.9,1.2,1.8,2.2,2.8,3.1]) for(const y of [-.5,-.25,0,.25,.5]) {
      const left=x<2.7,right=x>1.3;
      const alpha=(left?.55:0)+(right?.55:0)*(left?.45:1);
      const rgb=[0,1,2].map(c=>((right?[.95,.4,.1][c]*.55:0)+(left?[.2,.4,.9][c]*.55*(right?.45:1):0))*opacity);
      check([x,y+dy],[...rgb,alpha*opacity],[.7,-.7+dy,2.6,1.4],p.stage===0?{...burnRadial,seed:7}:cleanRadial);
    }
  }
  return {checks,maximumError,holeChecks,reference:'independent capture-bounds field and encoded premultiplied tint'};
}
const simpleTimes=[0,.5,.75,1,1.25,1.5,1.75,2.25,2.5,3,3.25,3.5,3.75];
const composedTimes=[0,.5,.75,1,1.25,1.5,1.75,2.25,2.5];
await qualifyPairedAuthoring({
  artifactDirectory:'browser-smoke-artifacts/radial-reveal',prepareContext:disableAuthoringJspi,
  cases:[...simpleTimes.map(sampleTime=>({
    id:`radial-reveal-${sampleTime}`,file:'radial_reveal.py',factory:'createDirectRadialRevealSmokeRenderer',
    duration:3.75,sampleTime,playback:'live',canvasSize:[640,360],objectCount:1,
    expectedBlank:[0,2.25,2.5].includes(sampleTime),directHeldSampleTime:({1.25:1,2.5:2.25,3.75:3.5})[sampleTime],
    boundaries:[1,1.25,2.25,2.5,3.5],stopAtSourceCompletion:sampleTime===3.75,
  })),...composedTimes.map(sampleTime=>({
    id:`radial-composed-${sampleTime}`,file:'radial_reveal_composed.py',factory:'createDirectRadialRevealComposedSmokeRenderer',
    composed:true,duration:2.5,sampleTime,playback:'live',canvasSize:[640,360],
    expectedBlank:[0,2.25,2.5].includes(sampleTime),directHeldSampleTime:({1.25:1,2.5:2.25})[sampleTime],
    boundaries:[1,1.25,2.25],stopAtSourceCompletion:sampleTime===2.5,
  }))],
  qualifyPixels({fixture,rust,python}){return {rust:reference(rust,fixture),python:reference(python,fixture)};},
});
