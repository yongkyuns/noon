import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import playwright from 'playwright';
import { PNG } from 'pngjs';
import { serveRepository } from './browser-test-server.mjs';
import { browserArgs } from './manim-raster-support.mjs';
import { createPyodideResourceCache } from './pyodide-resource-cache.mjs';
const root = process.cwd();
const out = path.resolve('dynamic-diagnostics');
await mkdir(out, {recursive:true});
const backend = process.env.NOON_DIAGNOSTIC_BACKEND ?? 'webgl';
const server = await serveRepository(root, 4199);
const browser = await playwright.chromium.launch({channel:'chromium',headless:true,args:browserArgs(backend)});
const context = await browser.newContext({viewport:{width:1000,height:650},deviceScaleFactor:1});
await createPyodideResourceCache(await readFile('web/python-worker.js','utf8')).install(context);
await context.route('**/dynamic-diagnostic.html', r => r.fulfill({contentType:'text/html',body:'<!doctype html><link rel="icon" href="data:,"><canvas id="scene" width="880" height="495" style="width:880px;height:495px"></canvas>'}));
const report = {backend, errors:[], comparisons:[]};
function diff(a,b) {
  a=PNG.sync.read(a);b=PNG.sync.read(b);assert.equal(a.width,b.width);assert.equal(a.height,b.height);
  let changed=0;for(let i=0;i<a.data.length;i+=4) if([0,1,2,3].some(c=>Math.abs(a.data[i+c]-b.data[i+c])>2)) changed++;
  return changed/(a.width*a.height);
}
async function start(source,pacing) {
  const page=await context.newPage();page.setDefaultTimeout(90000);
  page.on('pageerror',e=>report.errors.push(String(e)));
  page.on('console',m=>{if(m.type()==='error') report.errors.push(m.text());});
  await page.goto(`${server.baseUrl}/web/dynamic-diagnostic.html`);
  await page.evaluate(async ({source,pacing})=>{
    const {PythonAuthoringClient}=await import('./authoring-client.js');
    const {AuthoringExecutionClient}=await import('./authoring-execution-client.js');
    const authoring=new PythonAuthoringClient();const execution=new AuthoringExecutionClient(document.querySelector('#scene'));
    let resolve,reject;const attached=new Promise((a,b)=>{resolve=a;reject=b});
    const run=authoring.run(source,{}, {async onSemanticContinuation(r){
      await execution.prepare({transportMode:'transferable'});
      await execution.startSemanticExecution(r.semanticExecution,{authoringClient:authoring,loopDurationSeconds:r.duration,transportMode:'transferable',pacing});resolve();
    }});void run.catch(reject);window.h={authoring,execution,run};await attached;
  },{source,pacing});return page;
}
async function reconcile(page,complete=false) {
  return page.evaluate(async complete=>{
    if(complete) await h.execution.sampleToAuthoredTime(5.1,{stopAtSourceCompletion:true});
    const a=await h.run;
    await h.execution.reconcileSemanticExecution({contextId:a.semanticExecution.contextId,callbackSessionId:a.semanticExecution.callbackSessionId??null,continuationGeneration:null},{authoringClient:h.authoring,loopDurationSeconds:a.duration});
    await h.execution.pause();return a.duration;
  },complete);
}
try {
  const page=await start(await readFile('web/python/examples/manim_parity_stress_grid.py','utf8'),'external_samples');
  const times=[0.35,0.9,1.45,1.7,2.17,2.72,3.17,3.41,3.51,3.7,3.91,4.01,4.2,4.46,4.8,4.999];
  const original=new Map();
  for(const time of times) {
    await page.evaluate(t=>h.execution.sampleToAuthoredTime(t),time);
    original.set(time,await page.locator('#scene').screenshot({path:path.join(out,`source-${time}.png`)}));
    await writeFile(path.join(out,`source-${time}.json`),JSON.stringify(await page.evaluate(()=>h.execution.debugFrame())));
  }
  report.duration=await reconcile(page,true);
  for(const mode of ['seek','forward']) {
    await page.evaluate(()=>h.execution.seek(0));
    for(const time of (mode==='seek'?[...times].reverse():times)) {
      await page.evaluate(async({time,mode})=>{if(mode==='seek') await h.execution.seek(time);return h.execution.advanceTo(time);},{time,mode});
      const image=await page.locator('#scene').screenshot({path:path.join(out,`${mode}-${time}.png`)});
      const mismatch=diff(original.get(time),image);report.comparisons.push({mode,time,mismatch});console.log(mode,time,mismatch);
      await writeFile(path.join(out,`${mode}-${time}.json`),JSON.stringify(await page.evaluate(()=>h.execution.debugFrame())));
    }
  }
  await page.close();
  const waiting=await start('from noon import *\nclass WaitClock(Scene):\n    def construct(self):\n        dot=Circle(radius=0.4)\n        self.add(dot)\n        self.wait(2)\n        self.play(dot.animate.shift(RIGHT),run_time=0.4)\n        self.wait(2)\n        self.play(dot.animate.shift(LEFT),run_time=0.4)\n','realtime');
  report.waitSource=await waiting.evaluate(async()=>{
    const samples=[];const start=performance.now();
    while(performance.now()-start<1500){const now=performance.now();const state=await h.execution.state();samples.push({wall:(now-start)/1000,time:state.time,frameTime:(await h.execution.debugFrame()).time});await new Promise(r=>setTimeout(r,100));}return samples;
  });
  report.waitDuration=await reconcile(waiting);
  await waiting.evaluate(async()=>{await h.execution.seek(0);await h.execution.resume();});
  report.waitReplay=await waiting.evaluate(async()=>{
    const samples=[];const start=performance.now();
    while(performance.now()-start<1500){const now=performance.now();const state=await h.execution.state();samples.push({wall:(now-start)/1000,time:state.time,frameTime:(await h.execution.debugFrame()).time});await new Promise(r=>setTimeout(r,100));}return samples;
  });
  console.log('waitSource',report.waitSource);console.log('waitReplay',report.waitReplay);
  await waiting.close();
} catch(e) {report.failure=String(e.stack??e);throw e;}
finally {await writeFile(path.join(out,'report.json'),JSON.stringify(report,(_,v)=>typeof v==='bigint'?String(v):v,2));await browser.close();await server.close();}
