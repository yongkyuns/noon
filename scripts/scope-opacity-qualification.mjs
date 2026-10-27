// Literal primary-color rectangles and a full 2D Gaussian qualify composition.
// No engine bounds, transforms, uniforms, masks or filtered pixels are the oracle.
import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";
import { referenceRgbaGaussian } from "./gaussian-blur-reference.mjs";

const cache = new Map();
function reference(image, time) {
  assert.equal(image.width,640); assert.equal(image.height,360);
  const opacity = time < 0.5 ? 0.5 : time < 1 ? 0.25 : time < 1.5 ? 0 : time < 2.5 ? 1 : time < 4.5 ? 0.5 : 1;
  const outer = time < 4 ? 0.6 : 1;
  const blur = time >= 3 && time < 3.5;
  const key = `${opacity}/${outer}/${blur}`;
  let expected = cache.get(key);
  if (!expected) {
    let source = new Uint8Array(640*360*4);
    // Authored height6, 360 output pixels -> density60; centers(-.5,0),(.5,0), 2x2 rectangles.
    for (let y=120;y<240;y++) for (let x=230;x<410;x++) {
      source.set(x<290 ? [255,0,0,255] : x<350 ? [128,0,128,255] : [0,0,128,128],(y*640+x)*4);
    }
    if (blur) source = Uint8Array.from(referenceRgbaGaussian(source,640,360,1),v=>Math.round(255*v));
    expected = new Uint8Array(source.length);
    for (let at=0;at<source.length;at+=4) {
      for (let c=0;c<3;c++) {
        let value = source[at+c]*opacity;
        if (outer!==1) value = Math.round(value)*outer;
        expected[at+c] = Math.round(value);
      }
      expected[at+3]=255;
    }
    // The independent ordinary witness prevents an intentionally empty group
    // sample from qualifying as a wholly blank/failed renderer.
    for (let y=282;y<318;y++) for (let x=62;x<98;x++) expected.fill(255,(y*640+x)*4,(y*640+x)*4+4);
    cache.set(key,expected);
  }
  let maximumError=0;
  for (let at=0;at<expected.length;at++) maximumError=Math.max(maximumError,Math.abs(expected[at]-image.data[at]));
  assert.ok(maximumError<=2,`scope opacity t=${time}: maximum byte error ${maximumError}`);
  if (opacity===0) assert.equal(maximumError,0,"zero applies to the complete child composition");
  return {maximumError,opacity,outer,blur,channels:expected.length};
}
await qualifyPairedAuthoring({
  cases:[0.25,0.75,1.25,1.75,2.25,2.75,3.25,3.75,4.25,4.75,5].map(sampleTime=>({
    id:`scope-opacity-${sampleTime}`,file:"scope_opacity.py",factory:"createDirectScopeOpacitySmokeRenderer",
    duration:5,sampleTime,playback:"live",canvasSize:[640,360],
    directHeldSampleTime:sampleTime===5?undefined:Math.floor(sampleTime*2)/2,
    boundaries:[0.5,1,1.5,2,2.5,3,3.5,4,4.5],stopAtSourceCompletion:sampleTime===5,
  })),
  artifactDirectory:"browser-smoke-artifacts/scope-opacity",prepareContext:disableAuthoringJspi,
  qualifyPixels({fixture,rust,python}) {return {rust:reference(rust,fixture.sampleTime),python:reference(python,fixture.sampleTime)};},
});
