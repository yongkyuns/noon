import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";
import { filtered } from "./noise-paint-reference.mjs";

const profiles=[
  {seed:0,frequency:[0.125,0.125],phase:[0,0],size:[4,2]},
  {seed:7,frequency:[8,8],phase:[-0.173,0.231],size:[4,2]},
  {seed:4294967295,frequency:[32,12],phase:[65535.827,-65535.769],size:[4,2]},
  {seed:7,frequency:[256,256],phase:[-0.173,0.231],size:[4,2]},
  {seed:7,frequency:[256,256],phase:[-0.173,0.231],size:[0.5,0.5]},
  {seed:7,frequency:[8,8],phase:[4294967296.173,-4294967296.231],size:[4,2]},
  {seed:7,frequency:[8,8],phase:[-1e-20,0],size:[4,2]},
];
await qualifyPairedAuthoring({
  artifactDirectory:process.env.NOON_NOISE_ARTIFACTS??"browser-smoke-artifacts/noise-paint-authoring",
  cases:[...profiles.map((profile,index)=>({
    id:`noise-profile-${index}`,profile,factory:"createDirectNoisePaintProfileSmokeRenderer",factoryArgs:[index],
    objectCount:1,duration:0,sampleTime:0,canvasSize:[960,540],
    source:`from noon import Color, NoisePaint, Rectangle, Scene
class NoiseSurface(Scene):
    def construct(self):
        panel = Rectangle(width=${profile.size[0]},height=${profile.size[1]},stroke=None)
        panel.set_fill(NoisePaint(Color(1,0,0,0.5),Color(0,0,1),seed=${profile.seed},frequency=(${profile.frequency}),phase=(${profile.phase})),opacity=0.7)
        panel.set_object_opacity(0.6)
        self.add(panel)
`,
  })),...[0.25,1,1.5,2.25,2.75,3.5,4.25].map(sampleTime=>({
    id:`noise-paint-${sampleTime}`,file:"noise_paint.py",factory:"createDirectNoisePaintSmokeRenderer",
    objectCount:1,duration:4.25,sampleTime,playback:"live",directHeldSampleTime:sampleTime===0.25?0:undefined,
    boundaries:[0.5,1.5,1.75,2.75,3,4],stopAtSourceCompletion:sampleTime===4.25,
  }))],
  prepareContext:disableAuthoringJspi,
  qualifyPixels({fixture,rust,python}) {
    let checks=0,maximumByteError=0;
    const profile=fixture.profile,time=fixture.sampleTime;
    let seed=7,frequency,phase,low,high,opacity,size=profile?.size??[4,2];
    if(profile) {
      ({seed,frequency,phase}=profile);low=[0.5,0,0];high=[0,0,1];opacity=0.42;
    } else if(time<=1.5) {
      const a=Math.max(0,time-0.5);frequency=[8+24*a,8+4*a];phase=[-0.173+0.704*a,0.231-0.95*a];
      low=[0.5*(1-a),a,0];high=[0,0,1-a];opacity=1-a/2;
    } else if(time<3) {
      const a=Math.max(0,Math.min(1,time-1.75));frequency=[32,12];phase=[0.531,-0.719];
      low=[a,1-a,0];high=[a,0,0];opacity=0.5+a/2;
    } else {
      const a=Math.min(1,time-3);frequency=[8,8];phase=[-0.173,0.231];
      low=[1-a/2,0,0];high=[1-a,0,a];opacity=1;
    }
    for(const image of [rust,python]) {
      const scale=image.height/8, widths=frequency.map((v,i)=>v/size[i]/scale);
      for(const u of [0.17,0.38,0.61,0.83])for(const v of [0.19,0.47,0.79]) {
        const x=Math.floor(image.width/2+(u-0.5)*size[0]*scale),y=Math.floor(image.height/2-(v-0.5)*size[1]*scale);
        const uv=[((x+0.5-image.width/2)/scale)/size[0]+0.5,((image.height/2-y-0.5)/scale)/size[1]+0.5];
        const q=filtered(seed,uv,frequency,phase,widths);
        const expected=[...low.map((c,i)=>Math.floor(Math.floor((c*(1-q)+high[i]*q)*opacity*65535+0.5)/256)),255];
        const at=(y*image.width+x)*4,actual=image.data.subarray(at,at+4);
        const error=Math.max(...expected.map((v,i)=>Math.abs(v-actual[i])));
        maximumByteError=Math.max(maximumByteError,error);
        assert.ok(error<=1,`${fixture.id} (${x},${y}) ${[...actual]} != ${expected}; field=${q}`);
        checks++;
      }
    }
    return {interiorPixelComparisons:checks,maximumByteError,reference:"independent BigInt hash, piecewise Gaussian quadrature and physical-pixel lattice footprint; encoded LDR quantization"};
  },
});
