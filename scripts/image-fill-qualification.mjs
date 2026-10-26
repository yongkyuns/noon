// Direct Rust/WASM and ordinary Python image paints use the same retained renderer.
import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

const pixels = [[255,0,0,255],[0,255,255,0],[0,0,255,128],[240,100,10,64],[20,240,100,192],[150,20,240,255]];
function reference(u, v) {
  const position = [u * 3 - 0.5, v * 2 - 0.5];
  const base = position.map(Math.floor), fraction = position.map((p, i) => p - base[i]);
  const color = [0,0,0,0];
  for (let y = 0; y < 2; y++) for (let x = 0; x < 2; x++) {
    const p = pixels[Math.max(0,Math.min(1,base[1]+y))*3+Math.max(0,Math.min(2,base[0]+x))];
    const weight = (x ? fraction[0] : 1-fraction[0]) * (y ? fraction[1] : 1-fraction[1]);
    for (let c = 0; c < 4; c++) color[c] += weight * p[c] / 255 * (c === 3 ? 1 : p[3] / 255);
  }
  return color;
}

await qualifyPairedAuthoring({
  artifactDirectory: "browser-smoke-artifacts/image-fill-authoring",
  cases: [0.25,0.75,1,1.5,1.8,2.1,2.25].map(sampleTime => ({
    id: `image-fill-${sampleTime}`, file: "image_fill.py", factory: "createDirectImageFillSmokeRenderer",
    objectCount: 1, duration: 2.25, sampleTime, playback: "live", canvasSize: [640,360],
    boundaries: [0.5,1.5,1.75,2], stopAtSourceCompletion: sampleTime === 2.25,
    directHeldSampleTime: sampleTime === 0.25 ? 0 : sampleTime === 1.8 ? 1.75 : sampleTime === 2.1 ? 2 : undefined,
  })),
  prepareContext: disableAuthoringJspi,
  qualifyPixels({fixture, rust, python}) {
    const time = fixture.sampleTime;
    const a = Math.max(0, Math.min(1, time - 0.5));
    const translation = time >= 1.75 ? [-0.23,0.17] : [0.173*a,-0.231*a];
    const scale = time >= 1.75 ? [-0.71,0.58] : [1-0.37*a,1+0.37*a];
    const angle = time >= 1.75 ? -0.61 : 0.37*a;
    const opacity = time >= 1.75 ? 0.8 : 1-0.5*a;
    let checks = 0, maximumByteError = 0;
    for (const image of [rust,python]) {
      for (const dx of [-65,-30,-7,11,43,70]) for (const dy of [-30,-11,17,31]) {
        const x = image.width/2+dx, y = image.height/2+dy;
        const normalized = [((x+0.5-image.width/2)/(image.height/8)+2)/4,((image.height/2-y-0.5)/(image.height/8)+1)/2];
        const delta = [normalized[0]-0.5-translation[0],normalized[1]-0.5-translation[1]];
        const placed = [(Math.cos(angle)*delta[0]+Math.sin(angle)*delta[1])/scale[0]+0.5,(-Math.sin(angle)*delta[0]+Math.cos(angle)*delta[1])/scale[1]+0.5];
        let color = reference(placed[0], 1-(placed[1]*2+1/3)/(8/3));
        if (time >= 2) color = [0.2,0.4,0.8,1];
        const expected = [...color.slice(0,3).map(c => Math.floor(Math.floor(c*(time>=2 ? 1 : opacity)*65535+0.5)/256)),255];
        const at = (y*image.width+x)*4, actual = image.data.subarray(at,at+4);
        for (let c=0;c<4;c++) maximumByteError=Math.max(maximumByteError,Math.abs(actual[c]-expected[c]));
        assert.ok(expected.every((v,c)=>Math.abs(v-actual[c])<=1), `${fixture.id} (${x},${y}) ${[...actual]} != ${expected}`);
        checks++;
      }
    }
    return { interiorPixelComparisons: checks, maximumByteError,
      reference: "independent physical fitted rectangle, inverse placement, four premultiplied texels and Cairo LDR quantization" };
  },
});
