// Host/color qualification over ordinary shared Rust authoring, not a shader mock.
import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import playwright from 'playwright';
import { PNG } from 'pngjs';
import { serveRepository } from './browser-test-server.mjs';
import { browserArgs } from './manim-raster-support.mjs';
import { createRuntimeBuildIdentity } from './build-runtime-identity.mjs';
import { installWebGpuDeviceCapture } from './webgpu-device-capture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const {chromium}=playwright;
const output=process.env.NOON_BROWSER_HDR_ARTIFACTS ?? path.join(root,'ci-artifacts/direct-view-output');
await mkdir(output,{recursive:true});
const runtime=JSON.parse(await readFile(root+'/web/runtime-build-identity.json','utf8'));
assert.deepEqual(await createRuntimeBuildIdentity(root),runtime,'package must match clean source and every runtime file');
assert.ok(runtime.sourceRevision,'clean exact source required');
const report={scope:'direct-rust-authored-hdr',gpuMode:'software',status:'running',runtime,scriptSha256:createHash('sha256').update(await readFile(new URL(import.meta.url))).digest('hex'),cases:[]};
const decode=x=>x<=0.04045?x/12.92:((x+0.055)/1.055)**2.4;
const encode=x=>x<=0.0031308?12.92*x:1.055*x**(1/2.4)-0.055;
// The circle fully covers the convolution footprint at its center. This
// independent f64 reference checks the host material/transfer path; full field
// convolution and overlay negative controls remain in the native GPU oracle.
function reference(time){
 const phase=time<=0.5?0:time<2.5?(time-0.5)/2:time<=3?1:time<5?(5-time)/2:0;
 const linear=[0.2,0.4,0.8].map(x=>decode(x)*(1+7*phase));
 const luminance=linear.reduce((s,x,i)=>s+x*[0.2126,0.7152,0.0722][i],0);
 const extraction=Math.max(0,luminance-0.15)/luminance;
 return [...linear.map(x=>Math.round(255*encode((x*(1+extraction*3*phase))/(1+x*(1+extraction*3*phase))))),255];
}
async function recoverHdr(page, backend, label) {
 const before=await page.evaluate(()=>({time:hdr.renderer.time(),revision:String(hdr.renderer.directSceneRevision()),
  generation:hdr.renderer.gpuGeneration(),frame:JSON.parse(hdr.renderer.debugSelectionFrameJson())}));
 const original=PNG.sync.read(await page.locator('#scene').screenshot({scale:'css'}));
 if(backend==='webgpu') {
  await page.evaluate(()=>{
   const capture=globalThis.__noonWebGpuDeviceCapture,index=capture?.ownerDeviceIndex;
   if(index===null||index===undefined||!capture.devices[index]) throw new Error('HDR owner GPUDevice was not captured');
   hdr.lostIndex=index;capture.devices[index].destroy();
  });
  await page.waitForFunction(()=>globalThis.__noonWebGpuDeviceCapture.lost[hdr.lostIndex]!==null);
 } else {
  await page.evaluate(()=>{
   const extension=hdr.surface.getContext('webgl2')?.getExtension('WEBGL_lose_context');
   if(!extension) throw new Error('real HDR WebGL context loss is unavailable');
   hdr.loss={lost:0,restored:0,extension};
   hdr.surface.addEventListener('webglcontextlost',event=>{event.preventDefault();hdr.loss.lost++;},{once:true});
   hdr.surface.addEventListener('webglcontextrestored',()=>hdr.loss.restored++,{once:true});
   extension.loseContext();
  });
  await page.waitForFunction(()=>hdr.loss.lost===1);
  await page.evaluate(()=>hdr.loss.extension.restoreContext());
  await page.waitForFunction(()=>hdr.loss.restored===1);
 }
 const after=await page.evaluate(async backend=>{
  let recovered=false;
  for(let attempt=0;attempt<100&&!recovered;attempt++) {
   recovered=backend==='webgpu'?await hdr.renderer.recoverWebGpuDevice():await hdr.renderer.recoverWebGlContext();
   if(!recovered) await new Promise(resolve=>setTimeout(resolve,10));
  }
  if(!recovered) throw new Error('HDR GPU owner did not recover');
  for(let attempt=0;attempt<100;attempt++) {
   if(hdr.renderer.render()) return {time:hdr.renderer.time(),revision:String(hdr.renderer.directSceneRevision()),
    generation:hdr.renderer.gpuGeneration(),frame:JSON.parse(hdr.renderer.debugSelectionFrameJson())};
   await new Promise(resolve=>setTimeout(resolve,10));
  }
  throw new Error('recovered HDR frame did not present');
 },backend);
 assert.equal(after.generation,before.generation+1,'replace the actual GPU owner once');
 assert.equal(after.time,before.time,'GPU recovery cannot advance authored time');
 assert.equal(after.revision,before.revision,'GPU recovery cannot replace semantic state');
 assert.deepEqual(after.frame,before.frame,'GPU recovery preserves the complete authored pose');
 await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
 const bytes=await page.locator('#scene').screenshot({scale:'css'});await writeFile(`${output}/${label}-recovered.png`,bytes);
 const restored=PNG.sync.read(bytes);
 assert.equal(restored.width,original.width);assert.equal(restored.height,original.height);
 assert.deepEqual(restored.data,original.data,'recovered HDR/Bloom pixels are exact');
 const at=4*(Math.floor(restored.height/2)*restored.width+Math.floor(restored.width/2));
 const center=[...restored.data.subarray(at,at+4)],expected=reference(before.time);
 assert.ok(center.every((x,i)=>Math.abs(x-expected[i])<=2),'recovery retains the independent material/Bloom/transfer reference');
 return {label,before,after,center,expected,pixelsExact:true};
}
let browser,server;
try{
 server=await serveRepository(root,0,{crossOriginIsolated:true});
 for(const backend of ['webgpu','webgl']){
  browser=await chromium.launch({headless:true,channel:'chromium',args:browserArgs(backend)});
  const page=await browser.newPage({viewport:{width:800,height:500}});page.setDefaultTimeout(120000);
  if(backend==='webgl') await page.addInitScript(()=>Object.defineProperty(navigator,'gpu',{value:undefined}));
  else await installWebGpuDeviceCapture(page);
  const errors=[];page.on('pageerror',e=>errors.push(String(e)));
  for(const mode of ['session','program']) {
  if(mode==='program') { await page.evaluate(()=>hdr.renderer.free()); await page.reload(); }
  const entry={backend,mode,status:'running',frames:[],replay:[],recovery:[]};report.cases.push(entry);
  try{
   await page.goto(server.baseUrl+'/web/execution-worker-smoke.html');
   entry.actual=await page.evaluate(async mode=>{
    globalThis.Worker=class {constructor(){throw new Error('direct Rust attempted a Worker')}};
    const {default:init,createDirectCameraBloomRenderer,createDirectCameraBloomSessionRenderer}=await import('./pkg/noon_web.js');await init();
    const canvas=document.querySelector('#scene');
    const factory=mode==='session'?createDirectCameraBloomSessionRenderer:createDirectCameraBloomRenderer;
    const surface=canvas.transferControlToOffscreen();
    const renderer=await factory(surface);
    const {sampleDirectProgramThrough}=await import('../scripts/direct-program-sample.mjs');
    globalThis.hdr={renderer,surface,sample:time=>sampleDirectProgramThrough(renderer,time,[0.5,2.5,3,5])};
    return renderer.rendererBackend();
   },mode);
   assert.equal(entry.actual,backend==='webgpu'?'WebGPU':'WebGL2','actual renderer backend');
   const images=new Map();
   for(const time of [0,0.5,1.5,2.5,2.75,3,4,5]){
    const state=await page.evaluate(time=>hdr.sample(time),time);
    await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
    const bytes=await page.locator('#scene').screenshot({scale:'css'});await writeFile(`${output}/${backend}-${mode}-${time}.png`,bytes);
    const png=PNG.sync.read(bytes),at=4*(Math.floor(png.height/2)*png.width+Math.floor(png.width/2));
    const center=[...png.data.subarray(at,at+4)],expected=reference(time);
    assert.ok(center.every((x,i)=>Math.abs(x-expected[i])<=2),`${backend} t=${time}: center ${center} expected ${expected}`);
    assert.deepEqual([...png.data.subarray(0,4)],[0,0,0,255],'HDR clear is opaque black');
    const animating=(time>0.5&&time<2.5)||(time>3&&time<5);
    if(animating) assert.ok(Math.abs(state.time-time)<1e-12,`active authored time ${state.time} != ${time}`);
    else { assert.ok(state.time<=time); assert.deepEqual(reference(state.time),expected,'a sleeping publication must hold the requested authored pose'); }
    images.set(time,png.data);entry.frames.push({time,center,expected,state});
    if(time===1.5) entry.recovery.push(await recoverHdr(page,backend,`${backend}-${mode}-active`));
   }
   assert.deepEqual(images.get(0),images.get(5),'exact neutral material and Bloom restoration');
   if(mode==='program') {
    const rejection=await page.evaluate(()=>{try{hdr.renderer.seekDirect(1.5);return null;}catch(e){return String(e);}});
    assert.equal(rejection,'live continuation does not support arbitrary seek');entry.seekRejection=rejection;
    assert.equal(await page.evaluate(()=>hdr.renderer.time()),5);
    assert.deepEqual(PNG.sync.read(await page.locator('#scene').screenshot({scale:'css'})).data,images.get(5));
   } else for(const time of [1.5,5,0,2.5,0]){
    await page.evaluate(async time=>{hdr.renderer.seekDirect(time);hdr.renderer.render();await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));},time);
    const image=PNG.sync.read(await page.locator('#scene').screenshot({scale:'css'}));
    assert.deepEqual(image.data,images.get(time),`${backend} direct seek ${time} differs from authored forward frame`);entry.replay.push(time);
   }
   const beforeResize=PNG.sync.read(await page.locator('#scene').screenshot({scale:'css'})).data;
   const rejected=await page.evaluate(()=>{try{hdr.renderer.resize(3840,2160);return null;}catch(e){return String(e);}});
   assert.match(rejected,/budget|extent/,'over-budget resize must reject');
   assert.deepEqual(PNG.sync.read(await page.locator('#scene').screenshot({scale:'css'})).data,beforeResize,'rejected resize preserves output');
   for(const [width,height] of [[960,540],[640,360]]) {
    await page.evaluate(async ({width,height})=>{hdr.renderer.resize(width,height);hdr.renderer.render();await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));},{width,height});
    const png=PNG.sync.read(await page.locator('#scene').screenshot({scale:'css'}));
    const at=4*(Math.floor(png.height/2)*png.width+Math.floor(png.width/2));
    assert.ok([...png.data.subarray(at,at+4)].every((x,i)=>Math.abs(x-reference(0)[i])<=2),'resized neutral interior');
   }
   assert.deepEqual(PNG.sync.read(await page.locator('#scene').screenshot({scale:'css'})).data,beforeResize,'same-size restoration is exact');
   entry.resize={rejected,restored:true};
   entry.recovery.push(await recoverHdr(page,backend,`${backend}-${mode}-neutral`));
   assert.deepEqual(errors,[]);entry.status='passed';
  }catch(error){entry.status='failed';entry.error=String(error.stack??error);throw error;}
  finally{entry.errors=errors;}
  }
  await page.close();await browser.close();browser=null;
 }
 report.status='passed';
}catch(error){report.status='failed';report.error=String(error.stack??error);process.exitCode=1;}
finally{if(browser)await browser.close();if(server)await server.close();await writeFile(output+'/report.json',JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({status:report.status,source:runtime.sourceRevision,cases:report.cases.map(x=>({backend:x.backend,mode:x.mode,actual:x.actual,status:x.status,error:x.error})),output}));}
