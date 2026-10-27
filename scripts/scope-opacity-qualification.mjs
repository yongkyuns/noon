// Literal rectangles, scalar seeded fields and full 2D Gaussian composition.
// No engine bounds, transforms, uniforms or filtered pixels construct the oracle.
import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";
import { referenceRgbaGaussian } from "./gaussian-blur-reference.mjs";
import { dissolveCoverage } from "./dissolve-mask-reference.mjs";

const cache = new Map();
const exactBaselines = new Map();
function parameters(time) {
  let opacity = time < 0.5 ? 0.5 : time < 1 ? 0.25 : time < 1.5 ? 0 : time < 2.5 ? 1 : time < 4.5 ? 0.5 : 1;
  let outer = time < 4 ? 0.6 : 1;
  if (time >= 5 && time < 6) opacity = Math.fround(1 - 0.75 * (time - 5));
  else if (time >= 6 && time < 6.5) opacity = 0.25;
  else if (time >= 6.5 && time < 7.5) {
    opacity = Math.fround(0.25 + 0.5 * (time - 6.5));
    outer = Math.fround(1 - 0.5 * (time - 6.5));
  } else if (time >= 7.5 && time < 9.5) {
    opacity = 0.75; outer = 0.5;
    if (time >= 8 && time < 9) {
      const phase = time - 8;
      const mirrored = phase < 0.5 ? 2 * phase : 2 * (1 - phase);
      const sigmoid = x => 1 / (1 + Math.exp(-x));
      const error = sigmoid(-5);
      const smooth = Math.max(0, Math.min(1, (sigmoid(10 * (mirrored - 0.5)) - error) / (1 - 2 * error)));
      opacity = Math.fround(0.75 * (1 - smooth));
    }
  }
  const halo = time >= 10 && time < 13.5;
  if (halo) { opacity = 0.5; outer = 0.6; }
  const progress = time < 10.5 || time >= 13 ? 0 : time < 11.5 ? time - 10.5 : time < 12 ? 1 : 13 - time;
  return {opacity, outer, blur:time >= 3 && time < 3.5, halo, progress};
}
function reference(image, time, host) {
  assert.equal(image.width,640); assert.equal(image.height,360);
  const p = parameters(time), key = JSON.stringify(p);
  let expected = cache.get(key);
  if (!expected) {
    let source = new Uint8Array(640*360*4);
    const original = new Uint8Array(source.length);
    // Height6, 360 pixels -> density60; exact 2x2 rectangles centered at +/-0.5.
    for (let y=120;y<240;y++) for (let x=230;x<410;x++) {
      const at=(y*640+x)*4;
      const mask=dissolveCoverage((x+0.5-230)/180,(y+0.5-120)/120,{seed:7,softness:0.2,progress:p.progress});
      const rgba=x<290?[255,0,0,255]:x<350?[128,0,128,255]:[0,0,128,128];
      source.set(rgba.map(value=>Math.round(value*mask)),at);
      original.fill(Math.round(255*mask),at,at+4);
    }
    if (p.blur) source=Uint8Array.from(referenceRgbaGaussian(source,640,360,1),v=>Math.round(255*v));
    source=Uint8Array.from(source,v=>Math.round(v*p.opacity));
    const mask=Uint8Array.from(original,v=>Math.round(v*p.opacity));
    if (p.halo) {
      const blurred=referenceRgbaGaussian(mask,640,360,2);
      for (let at=0;at<source.length;at+=4) {
        const added=(1-source[at+3]/255)*blurred[at+3];
        source[at]=Math.round(255*Math.min(1,source[at]/255+added));
        source[at+3]=Math.round(255*Math.min(1,source[at+3]/255+added));
      }
    }
    expected=new Uint8Array(source.length);
    for (let at=0;at<source.length;at+=4) {
      for (let c=0;c<3;c++) expected[at+c]=Math.round(source[at+c]*p.outer);
      expected[at+3]=255;
    }
    // Unrelated ordinary content remains present even at exact empty endpoints.
    for (let y=282;y<318;y++) for (let x=62;x<98;x++) expected.fill(255,(y*640+x)*4,(y*640+x)*4+4);
    cache.set(key,expected);
  }
  let maximumError=0;
  for (let at=0;at<expected.length;at++) maximumError=Math.max(maximumError,Math.abs(expected[at]-image.data[at]));
  assert.ok(maximumError<=2,`scope opacity t=${time}: maximum byte error ${maximumError}`);
  if (p.opacity===0 || p.progress===1) assert.equal(maximumError,0,"empty applies to child color and ancestor silhouette");
  const baselineTime = time===8.75 ? 8.25 : time===8.875 ? 8.125
    : [9.75,13.75,14].includes(time) ? 4.75 : time===13.25 ? 10.25 : undefined;
  if (baselineTime!==undefined) {
    const baseline=exactBaselines.get(`${host}/${baselineTime}`);
    assert.ok(baseline,`required same-backend baseline ${host}/${baselineTime} must be captured`);
    assert.deepEqual(Buffer.from(image.data),baseline,`exact returning/restoration identity at ${host}/${time}`);
  }
  if ([4.75,8.125,8.25,10.25].includes(time)) exactBaselines.set(`${host}/${time}`,Buffer.from(image.data));
  return {maximumError,...p,channels:expected.length,exactBaselineTime:baselineTime};
}
function held(time) {
  if (time<5) return Math.floor(time*2)/2;
  for (const [start,end] of [[6,6.5],[7.5,8],[9,9.5],[9.5,10],[10,10.5],[11.5,12],[13,13.5],[13.5,14]]) {
    if (time>=start && time<end) return start;
  }
}
const times=[...new Set([0.25,0.75,1.25,1.75,2.25,2.75,3.25,3.75,4.25,4.75,5,
  5.125,5.25,5.5,5.75,6,6.25,6.5,6.75,7,7.25,7.5,7.75,8,8.125,8.25,8.5,8.75,8.875,9,9.25,9.5,9.75,
  10,10.25,10.5,10.75,11,11.25,11.5,11.75,12,12.25,12.5,12.75,13,13.25,13.5,13.75,14])];
await qualifyPairedAuthoring({
  cases:times.map(sampleTime=>({
    id:`scope-opacity-${sampleTime}`,file:"scope_opacity.py",factory:"createDirectScopeOpacitySmokeRenderer",
    duration:14,sampleTime,playback:"live",canvasSize:[640,360],directHeldSampleTime:held(sampleTime),
    boundaries:[0.5,1,1.5,2,2.5,3,3.5,4,4.5,5,6,6.5,7.5,8,9,9.5,10,10.5,11.5,12,13,13.5],
    stopAtSourceCompletion:sampleTime===14,
  })),
  artifactDirectory:"browser-smoke-artifacts/scope-opacity",prepareContext:disableAuthoringJspi,
  qualifyPixels({fixture,backend,rust,python}) {return {
    rust:reference(rust,fixture.sampleTime,`${backend}/rust-wasm`),
    python:reference(python,fixture.sampleTime,`${backend}/python`),
  };},
});
