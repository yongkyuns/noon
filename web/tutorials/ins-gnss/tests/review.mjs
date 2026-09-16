// Actual Noon/Pyodide browser qualification. No replacement renderer or Python mocks.
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { serveRepository } from '../../../../scripts/browser-test-server.mjs';
import { browserArgs } from '../../../../scripts/manim-raster-support.mjs';

const root=process.cwd();
const base='web/tutorials/ins-gnss';
const backend=process.env.NOON_TUTORIAL_BACKEND??'webgpu';
const output=`browser-smoke-artifacts/ins-gnss/${backend}`;
const source=await readFile(path.join(root,base,'scene.py'),'utf8');
const durations=[46.75,47.9,25.2,25.2,39.85,33.75,26.75,26.2,26.75,43.4,17.65,17.65,25.65,40.2];
const chapters=(process.env.NOON_TUTORIAL_CHAPTERS??'1,2,3,4,5,6,7,8,9,10,11,12,13,14').split(',').map(Number);
const server=await serveRepository(root,4199);
const browser=await chromium.launch({headless:true,args:browserArgs(backend)});
const reports=[];
await mkdir(output,{recursive:true});
try{
  for(const chapter of chapters){
    const errors=[],frames=[];
    const folder=path.join(output,String(chapter).padStart(2,'0'));
    await mkdir(folder,{recursive:true});
    const page=await browser.newPage({viewport:{width:1280,height:720},deviceScaleFactor:1});
    page.on('pageerror',e=>errors.push(String(e)));
    page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
    try{
      await page.goto(`${server.baseUrl}/web/agent-preview-host.html`);
      await page.waitForFunction(()=>window.noonAgentPreviewHost!==undefined);
      await page.locator('#scene').evaluate(c=>{c.width=1280;c.height=720;c.style.width='1280px';c.style.height='720px';});
      const initial=await page.evaluate(s=>window.noonAgentPreviewHost.open(s,600),`context={'chapter':${chapter}}\n${source}`);
      const duration=durations[chapter-1];
      const times=[0,0.25,0.55,5,8,12,16,20,24,28,34,38,42,46].filter(t=>t<duration-0.05);
      times.push(duration);
      for(const time of times){
        const state=time===0?initial:await page.evaluate(t=>window.noonAgentPreviewHost.sample(t),time);
        await page.locator('#scene').screenshot({path:path.join(folder,`${time.toFixed(3)}.png`)});
        frames.push(state);
        assert.ok(Math.abs(state.frame.publishedTime-time)<1e-5,'Exact authored time');
        assert.ok(state.frame.objectCount>0,'Nonempty semantic scene');
        assert.equal(state.frame.rendererBackend.toLowerCase().includes(backend==='webgpu'?'webgpu':'webgl'),true);
      }
      console.log(`PASS chapter ${chapter}: ${frames.length} actual frames`);
    }catch(error){errors.push(String(error));console.error(`FAIL chapter ${chapter}: ${error}`);}
    finally{
      await page.evaluate(()=>window.noonAgentPreviewHost?.close()).catch(()=>{});
      await page.close();
      const report={chapter,browser:browser.version(),backend,sourceSha256:createHash('sha256').update(source).digest('hex'),frames,errors};
      reports.push(report);
      await writeFile(path.join(folder,'report.json'),JSON.stringify(report,null,2));
    }
  }
}finally{
  await writeFile(path.join(output,'summary.json'),JSON.stringify(reports.map(({frames,...r})=>({...r,frameCount:frames.length})),null,2));
  await browser.close();await server.close();
}
assert.equal(reports.filter(r=>r.errors.length).length,0,'Every chapter must execute successfully');
