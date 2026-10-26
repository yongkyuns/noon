// Direct Rust/WASM and ordinary Python image paints use the same retained renderer.
import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

const pixels = [[255,0,0,255],[0,255,255,0],[0,0,255,128],[240,100,10,64],[20,240,100,192],[150,20,240,255]];
function reference(u, v, sampling = "linear", address = "clamp") {
  const tap = (x,y) => {
    if (address === "transparent" && (x<0 || x>=3 || y<0 || y>=2)) return [0,0,0,0];
    if (address === "repeat") { x=((x%3)+3)%3; y=((y%2)+2)%2; }
    else { x=Math.max(0,Math.min(2,x)); y=Math.max(0,Math.min(1,y)); }
    const p=pixels[y*3+x];
    return p.map((c,i)=>c/255*(i===3 ? 1 : p[3]/255));
  };
  if (sampling === "nearest") return tap(Math.floor(u*3),Math.floor(v*2));
  const position=[u*3-0.5,v*2-0.5], base=position.map(Math.floor);
  const fraction=position.map((p,i)=>p-base[i]);
  const weight=value=> {
    const t=Math.abs(value);
    if (sampling === "linear") return Math.max(0,1-t);
    if (t<=1) return t**3-2*t*t+1;
    return t<2 ? -(t**3)+5*t*t-8*t+4 : 0;
  };
  const color=[0,0,0,0];
  for(let y=-1;y<=2;y++) for(let x=-1;x<=2;x++) {
    const p=tap(base[0]+x,base[1]+y), w=weight(x-fraction[0])*weight(y-fraction[1]);
    for(let c=0;c<4;c++) color[c]+=w*p[c];
  }
  color[3]=Math.max(0,Math.min(1,color[3]));
  for(let c=0;c<3;c++) color[c]=Math.max(0,Math.min(color[3],color[c]));
  return color;
}

const profiles = Array.from({length:27}, (_,index) => ({
  index, fit:["stretch","cover","contain"][Math.floor(index/9)],
  address:["transparent","clamp","repeat"][Math.floor(index/3)%3],
  sampling:["nearest","linear","bicubic"][index%3],
}));
const staticCases = profiles.map(profile=> ({
  id:`image-fill-profile-${profile.index}`, profile, factory:"createDirectImageFillProfileSmokeRenderer",
  factoryArgs:[profile.index], objectCount:1, duration:0, sampleTime:0, canvasSize:[640,360],
  source:`from noon import ImageMapping, ImagePaint, Rectangle, Scene
class ImageSurface(Scene):
    def construct(self):
        panel = Rectangle(width=4, height=2, stroke=None)
        mapping = ImageMapping(fit="${profile.fit}", alignment=(0.23,0.71), address="${profile.address}", translation=(0.173,-0.231), scale=(-0.71,0.58), rotation=0.37)
        panel.set_fill(ImagePaint(${JSON.stringify([pixels.slice(0,3),pixels.slice(3)])}, mapping=mapping, sampling="${profile.sampling}"), opacity=0.7)
        panel.set_object_opacity(0.6)
        self.add(panel)
`,
}));

await qualifyPairedAuthoring({
  artifactDirectory: "browser-smoke-artifacts/image-fill-authoring",
  cases: [...staticCases, ...[0.25,0.75,1,1.5,1.8,2.1,2.25].map(sampleTime => ({
    id: `image-fill-${sampleTime}`, file: "image_fill.py", factory: "createDirectImageFillSmokeRenderer",
    objectCount: 1, duration: 2.25, sampleTime, playback: "live", canvasSize: [640,360],
    boundaries: [0.5,1.5,1.75,2], stopAtSourceCompletion: sampleTime === 2.25,
    directHeldSampleTime: sampleTime === 0.25 ? 0 : sampleTime === 1.8 ? 1.75 : sampleTime === 2.1 ? 2 : undefined,
  }))],
  prepareContext: disableAuthoringJspi,
  qualifyPixels({fixture, rust, python}) {
    const time = fixture.sampleTime;
    const a = Math.max(0, Math.min(1, time - 0.5));
    const profile = fixture.profile;
    const translation = profile ? [0.173,-0.231] : time >= 1.75 ? [-0.23,0.17] : [0.173*a,-0.231*a];
    const scale = profile ? [-0.71,0.58] : time >= 1.75 ? [-0.71,0.58] : [1-0.37*a,1+0.37*a];
    const angle = profile ? 0.37 : time >= 1.75 ? -0.61 : 0.37*a;
    const opacity = profile ? 0.7*0.6 : time >= 1.75 ? 0.8 : 1-0.5*a;
    let checks = 0, maximumByteError = 0;
    for (const image of [rust,python]) {
      for (const dx of [-65,-30,-7,11,43,70]) for (const dy of [-30,-11,17,31]) {
        const x = image.width/2+dx, y = image.height/2+dy;
        const normalized = [((x+0.5-image.width/2)/(image.height/8)+2)/4,((image.height/2-y-0.5)/(image.height/8)+1)/2];
        const delta = [normalized[0]-0.5-translation[0],normalized[1]-0.5-translation[1]];
        const placed = [(Math.cos(angle)*delta[0]+Math.sin(angle)*delta[1])/scale[0]+0.5,(-Math.sin(angle)*delta[0]+Math.cos(angle)*delta[1])/scale[1]+0.5];
        const fit=profile?.fit ?? "cover", alignment=profile ? [0.23,0.71] : [0.5,0.5];
        const size=fit === "stretch" ? [4,2] : fit === "cover" ? [4,8/3] : [3,2];
        const uv=[(placed[0]*4-(4-size[0])*alignment[0])/size[0],1-(placed[1]*2-(2-size[1])*alignment[1])/size[1]];
        let color = fit === "contain" && uv.some(v=>v<0 || v>1) ? [0,0,0,0]
          : reference(...uv,profile?.sampling,profile?.address);
        if (time >= 2) color = [0.2,0.4,0.8,1];
        const expected = [...color.slice(0,3).map(c => Math.floor(Math.floor(c*(time>=2 ? 1 : opacity)*65535+0.5)/256)),255];
        const at = (y*image.width+x)*4, actual = image.data.subarray(at,at+4);
        for (let c=0;c<4;c++) maximumByteError=Math.max(maximumByteError,Math.abs(actual[c]-expected[c]));
        assert.ok(expected.every((v,c)=>Math.abs(v-actual[c])<=1), `${fixture.id} (${x},${y}) ${[...actual]} != ${expected}`);
        checks++;
      }
    }
    return { interiorPixelComparisons: checks, maximumByteError,
      reference: "independent physical fitted rectangle, inverse placement, independent nearest/tent/cubic premultiplied reconstruction and Cairo LDR quantization" };
  },
});
