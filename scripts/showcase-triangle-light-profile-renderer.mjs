// #2056: compare the actual release renderer with a pinned release baseline.
// This never rewrites an engine identity, disables visuals, or changes playback.
import assert from 'node:assert/strict';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { summarizeProfile } from './showcase-triangle-light-profile.mjs';
const hash = b => createHash('sha256').update(b).digest('hex');
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(root, 'browser-smoke-artifacts/triangle-profile/renderer');
await mkdir(out, {recursive:true});
const {chromium} = await import('playwright');
const {PNG} = (await import('pngjs')).default;
const {serveRepository} = await import('./browser-test-server.mjs');
const {browserArgs} = await import('./manim-raster-support.mjs');
const {createPyodideResourceCache} = await import('./pyodide-resource-cache.mjs');
const code = await readFile(path.join(root,'web/python/examples/showcase_triangle_light.py'),'utf8');
// Both release engines run the same current measurement/capture harness. Its
// relative imports still load each engine's own immutable runtime modules.
const profileHarness = await readFile(path.join(root,'web/scene-perf.js'),'utf8');
function simple(scale) {
  const shape = scale === null ? '' : `        triangle = VMobject(stroke_width=0, fill_color=BLUE, fill_opacity=1)\n        triangle.set_points_as_corners([(-${scale}, -${scale}, 0), (${scale}, -${scale}, 0), (0, ${scale}, 0), (-${scale}, -${scale}, 0)])\n        self.add(triangle)\n`;
  return `from noon import *\nclass TriangleLightShowcase(Scene):\n    def construct(self):\n${shape}        dot = Dot(color=WHITE)\n        self.add(dot)\n        self.play(dot.animate.shift(RIGHT*2),run_time=8,rate_func=linear)\n`;
}
const variants = new Map([['dot',simple(null)],['small-fill',simple(.15)],['large-fill',simple(3)],['full',code]]);
const bases = {baseline:path.resolve(process.env.NOON_TRIANGLE_PROFILE_RUNTIME), candidate:root};
const identities = {};
const servers = {};
const args=browserArgs('webgl');
const browser=await chromium.launch({headless:true,args});
const report={purpose:'Paired release renderer attribution, not physical-GPU or ordinary-playback FPS',
  revision:execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim(),
  args,browser:browser.version(),profileHarnessSha256:hash(profileHarness),identities,cases:[],pixelComparisons:[]};
try {
  for(const [name,dir] of Object.entries(bases)) {
    identities[name]=JSON.parse(await readFile(path.join(dir,'web/runtime-build-identity.json'),'utf8'));
    servers[name]=await serveRepository(dir,0);
  }
  assert.equal(identities.baseline.sourceRevision,process.env.NOON_TRIANGLE_PROFILE_ENGINE_SHA);
  assert.equal(identities.candidate.sourceRevision,report.revision);
  const images=new Map();
  for(const [name,source] of variants) {
    for(const engine of ['baseline','candidate']) {
      const result={name,engine,sourceSha256:hash(source),outcome:'fail',pageErrors:[]};
      report.cases.push(result);
      const context=await browser.newContext({viewport:{width:1200,height:900},deviceScaleFactor:1});
      const cache=createPyodideResourceCache(await readFile(path.join(bases[engine],'web/python-worker.js'),'utf8'));
      await cache.install(context);
      const page=await context.newPage();
      page.on('pageerror',e=>result.pageErrors.push(String(e)));
      try {
        await page.route('**/web/scene-perf.js',r=>r.fulfill({contentType:'text/javascript',body:profileHarness}));
        await page.route('**/python/demo_scene.py',r=>r.fulfill({contentType:'text/plain',body:source}));
        const params=new URLSearchParams({source:'./python/demo_scene.py',warmup:'24',frames:'90',targetHz:'30',
          includeSamples:'1',includeRendererSamples:'1',includeStageTimings:'1',includeRendererGpuIdentity:'1',
          captureBeforeTerminate:'1'});
        await page.goto(servers[engine].baseUrl+'/web/scene-perf.html?'+params);
        await page.waitForFunction(()=>['complete','error'].includes(document.querySelector('#status')?.dataset.state),null,{polling:100,timeout:180000});
        const observed=await page.evaluate(()=>({state:document.querySelector('#status').dataset.state,status:document.querySelector('#status').value,report:window.__NOON_SCENE_PERF__}));
        await writeFile(path.join(out,name+'-'+engine+'.json'),JSON.stringify(observed,null,2));
        assert.equal(observed.state,'complete',observed.status);
        assert.deepEqual(result.pageErrors,[]);
        assert.deepEqual(observed.report.runtimeBuild,identities[engine]);
        result.summary=summarizeProfile(observed.report);
        const png=await page.locator('#scene').screenshot({path:path.join(out,name+'-'+engine+'.png')});
        const pixels=PNG.sync.read(png);
        // The border/background alone cannot qualify a rendered scene. Every
        // variant contains a visible white dot, independently of its fill.
        let visiblePixels=0;
        for(let y=2;y<pixels.height-2;y++)for(let x=2;x<pixels.width-2;x++){
          const offset=(y*pixels.width+x)*4;
          if(Math.max(...pixels.data.subarray(offset,offset+3))>100)visiblePixels++;
        }
        result.visiblePixels=visiblePixels;
        assert.ok(visiblePixels>0,'renderer capture contains no foreground pixels');
        result.pngSha256=hash(png);images.set(name+'-'+engine,pixels);
        result.outcome='pass';
        console.log('PASS renderer',name,engine,JSON.stringify(result.summary.cadence));
      } catch(e) {result.error=String(e.stack??e);console.error('FAIL renderer',name,engine,result.error);}
      finally {
        await page.evaluate(()=>window.__NOON_SCENE_PERF_RELEASE_CAPTURE__?.()).catch(()=>{});
        await context.close();await writeFile(path.join(out,'report.json'),JSON.stringify(report,null,2));
      }
    }
    const a=images.get(name+'-baseline'), b=images.get(name+'-candidate');
    if(a&&b){
      assert.equal(a.width,b.width);assert.equal(a.height,b.height);
      let changed=0,max=0,total=0;
      for(let i=0;i<a.data.length;i++){const d=Math.abs(a.data[i]-b.data[i]);if(d)changed++;max=Math.max(max,d);total+=d;}
      const comparison={name,width:a.width,height:a.height,changedChannels:changed,maxChannelError:max,meanChannelError:total/a.data.length};
      report.pixelComparisons.push(comparison);
      await writeFile(path.join(out,'report.json'),JSON.stringify(report,null,2));
      // Independent image guard, not a replacement for full gallery/replay gates.
      assert.ok(max<=2&&comparison.meanChannelError<.01,JSON.stringify(comparison));
    }
  }
} finally {
  await writeFile(path.join(out,'report.json'),JSON.stringify(report,null,2));
  await browser.close();for(const server of Object.values(servers))await server.close();
}
assert.ok(report.cases.every(row=>row.outcome==='pass'),'renderer comparison failed');
