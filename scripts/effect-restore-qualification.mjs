// Lifecycle integration: exact earlier frames plus independent interior alpha
// witnesses. Existing operator references separately qualify Blur/Glow filtering.
import assert from "node:assert/strict";
import {qualifyPairedAuthoring} from "./paired-authoring-qualification.mjs";
import {disableAuthoringJspi} from "./playground-browser-support.mjs";

const baselines = new Map();
function reference(image, time, host) {
  assert.equal(image.width,640); assert.equal(image.height,360);
  const pixels=Buffer.from(image.data), initial=baselines.get(host);
  const restored=time>=1.5 && time<2.5;
  if (time===0.25) baselines.set(host,pixels);
  else {
    assert.ok(initial,"initial frame must be captured first");
    if (restored) assert.ok(pixels.equals(initial),`restoration/saved-copy independence must be byte-exact: ${host} t=${time}`);
    else assert.ok(!pixels.equals(initial),`changed/removed appearance must differ: ${host} t=${time}`);
  }
  const opacity=time<0.5 || restored ? 0.6 : time<1.5 ? 0.25 : time<3 ? 0 : 1;
  let maximumInteriorError=0;
  // Interior source alpha is .6 for one white rectangle and .84 in their
  // overlap. Constant-field Blur preserves those values at these points.
  for(const [x,alpha] of [[410,0.84],[460,0.6]]) {
    const expected=Math.round(Math.round(255*alpha)*opacity), at=(180*640+x)*4;
    for(let channel=0;channel<3;channel++) maximumInteriorError=Math.max(maximumInteriorError,Math.abs(image.data[at+channel]-expected));
  }
  assert.ok(maximumInteriorError<=1,`independent group alpha t=${time}: ${maximumInteriorError}`);
  const witness=(270*640+140)*4;
  assert.deepEqual(Array.from(image.data.slice(witness,witness+4)),[255,255,255,255]);
  return {maximumInteriorError,opacity,exactRestoration:restored,channels:6};
}
const times=[0.25,0.5,0.75,1,1.25,1.5,1.75,2,2.25,2.5,2.75,3,3.25,3.5];
await qualifyPairedAuthoring({
  cases:times.map(sampleTime=>({id:`effect-restore-${sampleTime}`,file:"effect_restore.py",factory:"createDirectEffectRestoreSmokeRenderer",
    duration:3.5,sampleTime,playback:"live",canvasSize:[640,360],directHeldSampleTime:sampleTime===3.5?undefined:Math.floor(sampleTime*2)/2,
    boundaries:[0.5,1,1.5,2,2.5,3],stopAtSourceCompletion:sampleTime===3.5})),
  artifactDirectory:"browser-smoke-artifacts/effect-restore",prepareContext:disableAuthoringJspi,
  qualifyPixels({fixture,backend,rust,python}) {return {
    rust:reference(rust,fixture.sampleTime,`${backend}/rust-wasm`),
    python:reference(python,fixture.sampleTime,`${backend}/python`),
  };},
});
