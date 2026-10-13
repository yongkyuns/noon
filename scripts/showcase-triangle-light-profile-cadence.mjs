// #2056 diagnostic only. Never changes product pacing or claims hardware FPS.
// Separate browser rAF delivery from execution and asynchronous GPU completion.
import assert from 'node:assert/strict';
import {readFile, writeFile, mkdir} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
const hash = x => createHash('sha256').update(x).digest('hex');
export function instrumentProfiler(source, timer = false) {
  const anchor = 'function nextAnimationFrame() {\n  return new Promise((resolve) => requestAnimationFrame(resolve));\n}';
  assert.equal(source.split(anchor).length, 2, 'profiler pacing hook changed');
  assert.ok(source.includes('profileRenderSubstages: rendererSamples !== null,'));
  assert.ok(source.includes('metricsQueryMs: performance.now() - metricsStarted,'));
  return source.replaceAll('profileRenderSubstages: rendererSamples !== null,',
    'profileRenderSubstages: rendererSamples !== null, profileGpuCompletion: true,')
    .replace('metricsQueryMs: performance.now() - metricsStarted,',
      'metricsQueryMs: performance.now() - metricsStarted, gpuCompletion: renderer.gpuCompletion ?? null,')
    .replace(anchor, `function nextAnimationFrame() {
      const requestedAt = performance.now();
      return new Promise(resolve => {
        const received = timestamp => {
          (window.__TRIANGLE_WAKE_SAMPLES__ ??= []).push({requestedAt, callbackAt:performance.now(), timestamp});
          resolve(timestamp);
        };
        ${timer ? 'setTimeout(() => received(performance.now()), 16);' : 'requestAnimationFrame(received);'}
      });
    }`);
}
async function main() {
  const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const runtimeRoot=path.resolve(process.env.NOON_TRIANGLE_PROFILE_RUNTIME ?? root);
  const output=path.join(root,'browser-smoke-artifacts/triangle-profile/cadence');
  await mkdir(output,{recursive:true});
  const {chromium}=await import('playwright');
  const {serveRepository}=await import('./browser-test-server.mjs');
  const {browserArgs}=await import('./manim-raster-support.mjs');
  const {createPyodideResourceCache}=await import('./pyodide-resource-cache.mjs');
  const args=browserArgs('webgl');
  const browser=await chromium.launch({headless:true,args});
  const server=await serveRepository(runtimeRoot,0);
  const base=server.baseUrl+'/web';
  const runtime=JSON.parse(await readFile(path.join(runtimeRoot,'web/runtime-build-identity.json'),'utf8'));
  assert.equal(runtime.sourceRevision,process.env.NOON_TRIANGLE_PROFILE_ENGINE_SHA);
  const cache=createPyodideResourceCache(await readFile(path.join(runtimeRoot,'web/python-worker.js'),'utf8'));
  const source=await readFile(path.join(root,'web/python/examples/showcase_triangle_light.py'),'utf8');
  const originalHost=await readFile(path.join(runtimeRoot,'web/scene-perf.js'),'utf8');
  const report={purpose:'rAF/CPU/GPU attribution, not a playback or performance pass',runtime,args,browser:browser.version(),cases:[]};
  try {
    for(const kind of ['raf-only','webgl-clear','single-dot','full-raf','full-timer']) {
      const context=await browser.newContext({viewport:{width:1200,height:900},deviceScaleFactor:1});
      const page=await context.newPage();
      const result={kind,outcome:'fail',errors:[]}; report.cases.push(result);
      page.on('pageerror',e=>result.errors.push(String(e)));
      try {
        if(kind==='raf-only'||kind==='webgl-clear') {
          await page.route('**/cadence-control.html',r=>r.fulfill({contentType:'text/html',body:'<!doctype html><canvas width="1152" height="648"></canvas>'}));
          await page.goto(base+'/cadence-control.html');
          result.samples=await page.evaluate(async kind=>{
            const gl=kind==='webgl-clear'?document.querySelector('canvas').getContext('webgl2'):null;
            if(kind==='webgl-clear'&&!gl)throw Error('WebGL2 unavailable');
            const samples=[];
            for(let i=0;i<45;i++){
              const requestedAt=performance.now();
              const timestamp=await new Promise(resolve=>requestAnimationFrame(resolve));
              const callbackAt=performance.now();
              if(gl){gl.clearColor(i%2,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);gl.finish();}
              samples.push({requestedAt,timestamp,callbackAt,drawAndFinishMs:performance.now()-callbackAt});
            }
            return samples;
          },kind);
        } else {
          await cache.install(context);
          const code=kind==='single-dot'?'from noon import *\nclass SingleDot(Scene):\n    def construct(self):\n        dot=Dot()\n        self.add(dot)\n        self.play(dot.animate.shift(RIGHT*2),run_time=8,rate_func=linear)\n':source;
          const host=instrumentProfiler(originalHost,kind==='full-timer');
          result.sourceSha256=hash(code);result.hostSha256=hash(host);
          await writeFile(path.join(output,kind+'.py'),code);
          await page.route('**/python/demo_scene.py',r=>r.fulfill({contentType:'text/plain',body:code}));
          await page.route('**/scene-perf.js',r=>r.fulfill({contentType:'text/javascript',body:host}));
          const params=new URLSearchParams({source:'./python/demo_scene.py',warmup:'24',frames:'90',targetHz:'30',includeSamples:'1',includeRendererSamples:'1',includeStageTimings:'1',includeRendererGpuIdentity:'1'});
          await page.goto(base+'/scene-perf.html?'+params);
          await page.waitForFunction(()=>['complete','error'].includes(document.querySelector('#status')?.dataset.state),null,{polling:100,timeout:180000});
          result.observed=await page.evaluate(()=>({state:document.querySelector('#status').dataset.state,status:document.querySelector('#status').value,report:window.__NOON_SCENE_PERF__,wakeSamples:window.__TRIANGLE_WAKE_SAMPLES__}));
          assert.equal(result.observed.state,'complete',result.observed.status);
          assert.deepEqual(result.observed.report.runtimeBuild,runtime);
        }
        assert.deepEqual(result.errors,[]);result.outcome='pass';
        console.log('PASS cadence',kind,result.observed?.report.cadence);
      } catch(e){result.error=String(e.stack??e);console.error('FAIL cadence',kind,result.error);}
      finally {await context.close();await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2));}
    }
  } finally {await browser.close();await server.close();}
  assert.ok(report.cases.every(x=>x.outcome==='pass'),'cadence diagnostics failed; inspect artifacts');
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))await main();
