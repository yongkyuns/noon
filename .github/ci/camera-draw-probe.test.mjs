import test from 'node:test';
import assert from 'node:assert/strict';
import { classifyFragment, installDrawProbe } from './camera-draw-probe.js';

function fixture(mode, kind = 'line', enabled = false) {
  class GL {
    constructor() { this.calls=[]; this.SCISSOR_TEST=3089; this.SHADER_TYPE=35663; this.FRAGMENT_SHADER=35632; this.VERTEX_SHADER=35633; this.drawingBufferWidth=622; this.drawingBufferHeight=350; }
    getShaderParameter(s) { return s.type; }
    shaderSource() {} attachShader() {} linkProgram() {} useProgram() {}
    enable(...a) {this.calls.push(['enable',...a]);} disable(...a) {this.calls.push(['disable',...a]);}
    scissor(...a) {this.calls.push(['scissor',...a]);}
    drawArrays(...a) {this.calls.push(['drawArrays',...a]); return 42;}
    drawElements(...a) {this.calls.push(['drawElements',...a]); return 42;}
    drawArraysInstanced(...a) {this.calls.push(['drawArraysInstanced',...a]); return 42;}
    drawElementsInstanced(...a) {this.calls.push(['drawElementsInstanced',...a]); return 42;}
    drawRangeElements(...a) {this.calls.push(['drawRangeElements',...a]); return 42;}
  }
  const probe=installDrawProbe({WebGL2RenderingContext:GL},mode), gl=new GL();
  const fragment={type:gl.FRAGMENT_SHADER},vertex={type:gl.VERTEX_SHADER}, program={};
  const body = {line:'float half_length=1.;', path:'revealed_path_color();',text:'sample_glyph();',present:'srgb_to_linear();'}[kind];
  gl.shaderSource(fragment,`#version 300 es\nvoid main() { ${body} }`);
  gl.shaderSource(vertex,'void main() { gl_Position=vec4(0.); }');
  gl.attachShader(program,fragment);gl.attachShader(program,vertex);gl.linkProgram(program);gl.useProgram(program);
  if (enabled) gl.enable(gl.SCISSOR_TEST);
  gl.scissor(10,20,300,250);gl.calls=[];
  return {probe,gl};
}
for (const method of ['drawArrays','drawElements','drawArraysInstanced','drawElementsInstanced','drawRangeElements']) {
  test(`normal ${method} forwards arguments and return unchanged`,()=>{
    const {gl,probe}=fixture('normal'); assert.equal(gl[method](4,0,6,2),42);
    assert.deepEqual(gl.calls,[[method,4,0,6,2]]);
    assert.equal(probe.snapshot().counters[0].submitted,1);
  });
}
for (const kind of ['line','path','text','present']) {
  test(`skip-${kind} suppresses only its matched pipeline`,()=>{
    const {gl,probe}=fixture(`skip-${kind}`,kind);gl.drawArrays(4,0,6);
    assert.deepEqual(gl.calls,[]);assert.equal(probe.snapshot().counters[0].skipped,1);
    const other=fixture(`skip-${kind}`,kind==='line'?'text':'line');
    other.gl.drawArrays(4,0,6);assert.equal(other.gl.calls.length,1);
  });
}
for (const enabled of [false,true]) {
  test(`empty scissor forwards draw and restores ${enabled?'enabled':'disabled'} original state`,()=>{
    const {gl,probe}=fixture('clip-all','line',enabled);gl.drawArrays(4,0,6);
    assert.deepEqual(gl.calls,[['enable',3089],['scissor',0,0,0,0],['drawArrays',4,0,6],['scissor',10,20,300,250],...(!enabled?[['disable',3089]]:[])]);
    assert.equal(probe.snapshot().counters[0].clipped,1);
  });
}
test('ordinary metrics cannot consume shader evidence',()=>{
  const {probe}=fixture('normal');assert.deepEqual(probe.snapshot().newPrograms,[]);
  const first=probe.snapshot(true);assert.equal(first.newPrograms.length,1);assert.match(first.newPrograms[0].fragment,/half_length/);
  assert.equal(probe.snapshot(true).newPrograms.length,0);
});
test('classification uses entry body, not unused helpers',()=>{
  assert.equal(classifyFragment('void sample_glyph() {}\nvoid main() {float half_length=1.;}'),'line');
  assert.equal(classifyFragment('void main() {head_angle=1.;}'),'circle');
  assert.equal(classifyFragment('void main() {half_size=1.;}'),'rectangle');
  assert.equal(classifyFragment('void main() {unknown();}'),'unknown');
});
test('bad mode and missing WebGL fail closed',()=>{
  assert.throws(()=>installDrawProbe({},'normal'),/requires WebGL2/);
  assert.throws(()=>installDrawProbe({},'skip-unknown'),/invalid/);
});
