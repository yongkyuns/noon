import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";
import { filtered } from "./noise-paint-reference.mjs";

const profiles = [
  {seed:7, frequency:[8,8], octaves:4, gain:0.5, size:[4,2]},
  {seed:4294967295, frequency:[4,5], octaves:5, gain:0.6, size:[4,2], ramp:true},
  {seed:7, frequency:[1,1], octaves:8, gain:0.6, size:[0.7,0.5]},
  {seed:7, frequency:[8,8], octaves:1, gain:0.5, size:[4,2]},
];
const basicStops = [[0,[1,0,0,0.5]],[1,[0,0,1,1]]];
const rampStops = [[0,[1,0,0,0]],[0.4,[0,1,0,1]],[1,[0,0,1,1]]];
const cloudStops = [[0,[0.05,0.10,0.25,1]],[1,[0.50,0.80,1,1]]];
const paperStops = [[0,[0.86,0.80,0.68,1]],[0.45,[0.96,0.91,0.81,1]],[1,[1,0.97,0.90,1]]];

function fractal(seed, uv, frequency, phase, widths, octaves, gain) {
  let value=0, weightSum=0;
  for(let i=0;i<octaves;i++) {
    const scale=2**i, weight=gain**i, octaveSeed=(seed+i*0x9e3779b9)>>>0;
    value+=weight*filtered(octaveSeed,uv,frequency.map(v=>v*scale),phase.map(v=>v*scale),widths.map(v=>v*scale));
    weightSum+=weight;
  }
  return value/weightSum;
}
function colorAt(stops,q) {
  const premul=c=>[c[0]*c[3],c[1]*c[3],c[2]*c[3],c[3]];
  if(q<stops[0][0])return premul(stops[0][1]);
  for(let i=1;i<stops.length;i++)if(q<stops[i][0]) {
    const a=stops[i-1],b=stops[i],t=(q-a[0])/(b[0]-a[0]),ca=premul(a[1]),cb=premul(b[1]);
    return ca.map((v,j)=>v*(1-t)+cb[j]*t);
  }
  return premul(stops.at(-1)[1]);
}
await qualifyPairedAuthoring({
  artifactDirectory:process.env.NOON_FRACTAL_ARTIFACTS??"browser-smoke-artifacts/fractal-noise-authoring",
  cases:[...profiles.map((profile,index)=>({
    id:`fractal-profile-${index}`,profile,factory:"createDirectFractalNoiseProfileSmokeRenderer",factoryArgs:[index],
    objectCount:1,duration:0,sampleTime:0,canvasSize:[960,540],
    source:`from noon import Color, NoisePaint, Rectangle, Scene
class FractalProfile(Scene):
    def construct(self):
        panel=Rectangle(width=${profile.size[0]},height=${profile.size[1]},stroke=None)
        paint=NoisePaint.fractal(Color(1,0,0,0.5),Color(0,0,1),seed=${profile.seed},frequency=(${profile.frequency}),phase=(-0.173,0.231),octaves=${profile.octaves},gain=${profile.gain}${profile.ramp?",stops=[(0,Color(1,0,0,0)),(0.4,Color(0,1,0)),(1,Color(0,0,1))]":""})
        panel.set_fill(paint,opacity=0.7)
        panel.set_object_opacity(0.6)
        self.add(panel)
`,
  })),...[0.25,1,2,3.5,4].map(sampleTime=>({
    id:`fractal-live-${sampleTime}`,file:"fractal_noise.py",factory:"createDirectFractalNoiseSmokeRenderer",
    objectCount:3,duration:4,sampleTime,playback:"live",directHeldSampleTime:sampleTime===0.25?0:undefined,
    boundaries:[0.5,3.5],stopAtSourceCompletion:sampleTime===4,
  }))],
  prepareContext:disableAuthoringJspi,
  qualifyPixels({fixture,rust,python}) {
    let checks=0,maximumByteError=0;
    const p=fixture.profile;
    const progress=Math.max(0,Math.min(1,(fixture.sampleTime-0.5)/3));
    const planes=p?[{...p,phase:[-0.173,0.231],center:[0,0],opacity:0.42,stops:p.ramp?rampStops:basicStops}]:[
      {seed:7,frequency:[8,8],phase:[1.5*progress,0],octaves:4,gain:0.5,size:[3.6,2.4],center:[-2.1,0.4*progress],opacity:1,stops:cloudStops},
      {seed:7,frequency:[18,18],phase:[0,0],octaves:4,gain:0.5,size:[3.6,2.4],center:[2.1,0],opacity:1,stops:paperStops},
      {seed:7,frequency:[8,8],phase:[0,0],octaves:4,gain:0.5,size:[1.4,0.45],center:[-2.1,-2.3],opacity:1,stops:cloudStops},
    ];
    for(const image of [rust,python]) for(const plane of planes) {
      const scale=image.height/8,widths=plane.frequency.map((v,i)=>v/plane.size[i]/scale);
      for(const u of [0.17,0.38,0.61,0.83])for(const v of [0.19,0.47,0.79]) {
        const x=Math.floor(image.width/2+(plane.center[0]+(u-0.5)*plane.size[0])*scale);
        const y=Math.floor(image.height/2-(plane.center[1]+(v-0.5)*plane.size[1])*scale);
        const uv=[((x+0.5-image.width/2)/scale-plane.center[0])/plane.size[0]+0.5,((image.height/2-y-0.5)/scale-plane.center[1])/plane.size[1]+0.5];
        const q=fractal(plane.seed,uv,plane.frequency,plane.phase,widths,plane.octaves,plane.gain);
        const color=colorAt(plane.stops,q);
        const expected=[...color.slice(0,3).map(c=>Math.floor(Math.floor(c*plane.opacity*65535+0.5)/256)),255];
        const at=(y*image.width+x)*4,actual=image.data.subarray(at,at+4);
        const error=Math.max(...expected.map((v,i)=>Math.abs(v-actual[i])));
        maximumByteError=Math.max(maximumByteError,error);
        assert.ok(error<=1,`${fixture.id} (${x},${y}) ${[...actual]} != ${expected}; field=${q}`);
        checks++;
      }
    }
    return {interiorPixelComparisons:checks,maximumByteError,reference:"independent masked-integer/quadrature noise per octave, normalized scalar accumulation, premultiplied color ramp and LDR quantization"};
  },
});
