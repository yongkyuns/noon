// Predicate unit tests with synthetic buffers. These do not render Noon.
import test from 'node:test';
import assert from 'node:assert/strict';
import {checkHighlight,checkSample,checkTextRegions} from './cuda-transpose-pixels.mjs';
function image() {
  const data=new Uint8Array(1280*720*4);
  for(let i=0;i<data.length;i+=4) data.set([21,31,49,255],i);
  return {width:1280,height:720,data};
}
function paint(im,center,w,h,rgb) {
  const u=im.height/8;
  for(let y=Math.ceil(im.height/2-(center[1]+h/2)*u);y<=Math.floor(im.height/2-(center[1]-h/2)*u);y++)
    for(let x=Math.ceil(im.width/2+(center[0]-w/2)*u);x<=Math.floor(im.width/2+(center[0]+w/2)*u);x++)
      if(x>=0&&x<im.width&&y>=0&&y<im.height) im.data.set([...rgb,255],(y*im.width+x)*4);
}
function highlight(im, intensity=0.20) {
  const y=2.09;
  paint(im,[-3.48,y],6.14,0.32,[104,131,155].map((x,i)=>Math.round(x*intensity+[21,31,49][i]*(1-intensity))));
  paint(im,[-4,y],0.6,0.10,[200,210,220]);
  return {read_src:{alpha:intensity,center:[-3.48,y],width:6.14,height:0.32,
    text_center:[-4,y],text_width:2,text_height:0.15,text:'tile[...]'}};
}
function packets(im) {
  const expected=Array.from({length:32},(_,lane)=>({lane,center:[0.7+lane%8*0.72,1.45-Math.floor(lane/8)*0.6],
    width:0.46,height:0.24,rgb:[103,218,238],has_label:true}));
  for(const p of expected) {
    paint(im,p.center,p.width,p.height,p.rgb);
    paint(im,p.center,0.07,0.12,[12,18,32]);
  }
  return {kind:'bank',expected_source:expected,expected_target:expected,
    observed_source:expected.map(p => ({label:{center:p.center,width:0.07,height:0.12}})),
    highlights:highlight(im)};
}
test('20% active-line contrast and glyph coverage pass the pixel predicate',()=>{
  const im=image(); assert.equal(checkHighlight(im,highlight(im)).length,1);
});
test('an almost invisible active row is rejected even with valid alpha metadata',()=>{
  const im=image(); const h=highlight(im,0.01);
  assert.throws(()=>checkHighlight(im,h),/CUDA_PIXELS\[highlight\]/);
});
test('missing glyph pixels are not accepted based on semantic text metadata',()=>{
  const im=image(); const h=highlight(im); paint(im,[-4,2.09],2,0.16,[38,51,70]);
  assert.throws(()=>checkHighlight(im,h),/CUDA_PIXELS\[text\]/);
});
test('all 32 endpoint packet regions and label regions are checked',()=>{
  const im=image(),event=packets(im); const r=checkSample(im,event,1);
  assert.equal(r.checked,32);assert.equal(r.labels,32);
});
test('a missing packet fails even if all metadata still describes the correct scene',()=>{
  const im=image(),event=packets(im);paint(im,event.expected_target[9].center,0.47,0.25,[21,31,49]);
  assert.throws(()=>checkSample(im,event,1),/CUDA_PIXELS\[packet\]/);
});
test('a detached invisible label is rejected independently of body color',()=>{
  const im=image(),event=packets(im);const p=event.expected_target[9];paint(im,p.center,p.width,p.height,p.rgb);
  assert.throws(()=>checkSample(im,event,1),/CUDA_PIXELS\[label\]/);
});
test('small glyph coverage is measured inside the projected label, not its packet',()=>{
  const im=image(),event=packets(im);
  const p=event.expected_target[0];
  paint(im,p.center,p.width,p.height,p.rgb);
  // A sparse antialiased glyph has two dark pixels in a much larger packet.
  // Its ink bounds, like the real captured 0 digit, are about four by seven pixels.
  event.observed_source[0].label.width=4/90;
  event.observed_source[0].label.height=7/90;
  const x=Math.round(im.width/2+p.center[0]*90);
  const y=Math.round(im.height/2-p.center[1]*90);
  for(const dy of [0,1]) im.data.set([26,49,64,255],((y+dy)*im.width+x)*4);
  assert.equal(checkSample(im,event,1).labels,32);
  paint(im,p.center,p.width,p.height,p.rgb);
  assert.throws(()=>checkSample(im,event,1),/CUDA_PIXELS\[label\]/);
});
test('antialiased glyphs retain visible contrast while faint ink is rejected',()=>{
  const im=image(),event=packets(im),p=event.expected_target[0];
  const ink=[12,18,32];
  for(const [coverage,visible] of [[0.7,true],[0.3,false]]) {
    paint(im,p.center,p.width,p.height,p.rgb);
    paint(im,p.center,0.07,0.12,p.rgb.map((v,i)=>Math.round(v+(ink[i]-v)*coverage)));
    if(visible) assert.equal(checkSample(im,event,1).labels,32);
    else assert.throws(()=>checkSample(im,event,1),/CUDA_PIXELS\[label\]/);
  }
});
test('endpoint overlap cannot be excused as a transient crossing',()=>{
  const im=image(),event=packets(im);event.expected_target[9].center=event.expected_target[8].center;
  assert.throws(()=>checkSample(im,event,1),/CUDA_PIXELS\[overlap\]/);
});
test('bank labels require rendered glyph coverage',()=>{
  const im=image();assert.throws(()=>checkTextRegions(im,[{center:[2,1],width:0.4,height:0.15,text:'B12'}]),/CUDA_PIXELS\[text\]/);
});
