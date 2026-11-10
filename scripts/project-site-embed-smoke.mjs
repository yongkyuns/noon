import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, firefox, webkit } from 'playwright';
import pngjs from 'pngjs';
import { serveRepository } from './browser-test-server.mjs';
import { playgroundLaunchOptions } from './playground-browser-support.mjs';
import { createPyodideResourceCache } from './pyodide-resource-cache.mjs';

// Real composed-host qualification. No stubbed main, editor, workers or canvas.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = path.join(root, 'browser-smoke-artifacts/project-site/embedded');
await mkdir(output, {recursive:true});
const server = await serveRepository(root, 0);
const selection = JSON.parse(await readFile(path.join(root,'web/site/examples.json'),'utf8'));
const report = [];
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const editSource = color => `from noon import *
class EmbedEdit(Scene):
    def construct(self):
        square = Square(color=${color})
        square.set_fill(${color}, opacity=1)
        self.play(Create(square), run_time=0.5)
        self.wait(2)
`;
async function workersRetired(workers) {
  const until = Date.now()+15000;
  while (workers.size && Date.now()<until) await new Promise(resolve=>setTimeout(resolve,25));
  assert.equal(workers.size,0,'navigation must retire all observed embedded workers');
}
function coloredPixels(bytes, minimumBuckets = 8) {
  const png = pngjs.PNG.sync.read(bytes), buckets = new Set();
  for (let i=0;i<png.data.length;i+=16) {
    const [r,g,b] = png.data.subarray(i,i+3); buckets.add(`${r>>4},${g>>4},${b>>4}`);
  }
  assert.ok(png.width>100 && png.height>60,'actual canvas dimensions');
  assert.ok(buckets.size>minimumBuckets,'canvas is not a flat placeholder');
  return {width:png.width,height:png.height,colorBuckets:buckets.size};
}
async function ready(frame, afterGeneration = 0) {
  await frame.waitForFunction(previous => {
    const gallery = window.__noonExampleGallery;
    if (!gallery || gallery.generationDiagnostics.runGeneration<=previous) return false;
    if (!gallery.runInFlight && document.querySelector('#patch-status')?.dataset.state==='error') {
      throw new Error(document.querySelector('#python-traceback')?.textContent || 'Noon source failed');
    }
    return gallery && gallery.generationDiagnostics.runGeneration>previous && !gallery.runInFlight &&
      document.querySelector('#draft-status')?.dataset.state==='applied' &&
      document.querySelector('#patch-status')?.dataset.state==='applied' &&
      document.querySelector('#status')?.dataset.playbackControls==='available' &&
      !document.querySelector('.playback-scrubber')?.disabled;
  },afterGeneration);
}
async function sample(frame, prefix, time, minimumBuckets = 8) {
  // The regular playback controls remain the authority even though the embed
  // hides their toolbar. Use the same input events, not an alternative renderer.
  if (await frame.locator('#status').getAttribute('data-playback-playing')==='true') {
    await frame.locator('.playback-toggle').evaluate(button=>button.click());
  }
  await frame.waitForFunction(()=>document.querySelector('#status').dataset.playbackPlaying==='false');
  await frame.locator('.playback-scrubber').evaluate((element,time)=>{
    element.value=String(time);element.dispatchEvent(new Event('input',{bubbles:true}));
    element.dispatchEvent(new Event('change',{bubbles:true}));
  },time);
  await frame.waitForFunction(time=>Math.abs(parseFloat(document.querySelector('#metric-time').value)-time)<.2,time);
  const bytes = await frame.locator('#scene').screenshot();
  const pixels = coloredPixels(bytes,minimumBuckets);
  await writeFile(path.join(output,`${prefix}.png`),bytes);
  return {target:time,time:await frame.locator('#metric-time').evaluate(element=>element.value),sha256:hash(bytes),...pixels};
}
async function edit(frame, page, source) {
  await frame.getByRole('tab',{name:'Code',exact:true}).click();
  const editor = frame.locator('.cm-content');
  await editor.waitFor({state:'visible'});
  await editor.click();
  await editor.press('ControlOrMeta+A');
  await page.keyboard.insertText(source);
}
try {
  for (const [name, engine] of Object.entries({chromium,firefox,webkit})) {
    const browser = await engine.launch(playgroundLaunchOptions(name));
    try {
      const context = await browser.newContext({viewport:{width:1280,height:1000},colorScheme:'dark'});
      const cache = createPyodideResourceCache(await readFile(path.join(root,'_site/python-worker.js'),'utf8'));
      await cache.install(context);
      // Passive observation starts before any frame scripts, so a short scene
      // finishing while its siblings load still proves that autoplay occurred.
      await context.addInitScript(() => {
        window.__noonSitePlayingObserved = false;
        const observer = new MutationObserver(() => {
          if (document.querySelector('#status')?.dataset.playbackPlaying === 'true') {
            window.__noonSitePlayingObserved = true;
            observer.disconnect();
          }
        });
        observer.observe(document, {subtree:true,attributes:true,attributeFilter:['data-playback-playing']});
        window.addEventListener('pagehide',()=>observer.disconnect(),{once:true});
      });
      const page = await context.newPage();
      page.setDefaultTimeout(120000);
      const errors=[], workers=new Set();
      page.on('pageerror',error=>errors.push(String(error)));
      page.on('worker',worker=>{workers.add(worker);worker.on('close',()=>workers.delete(worker));});
      for (const [surface, route] of [['home','index.html'],['docs','site/docs/examples.html']]) {
        const start = report.length;
        try {
          await page.goto(`${server.baseUrl}/_site/${route}`);
          await page.waitForFunction(count=>document.querySelectorAll('iframe[data-shell-ready=true]').length===count,selection.length);
          const frames = await Promise.all(selection.map(async ({id}) => {
            const element = await page.locator(`[data-demo="${id}"] iframe`).elementHandle();
            return {id,element,frame:await element.contentFrame()};
          }));
          // No Run, open-preview or Play gesture: every scene must start itself.
          await Promise.all(frames.map(({frame})=>ready(frame)));
          assert.ok(workers.size>=selection.length,'all live contexts created runtime workers');
          assert.equal(await page.locator('video, .demo-poster, [data-demo-open]').count(),0);
          for (const [index,{id,element,frame}] of frames.entries()) {
            await page.setViewportSize({width:index%2 ? 390 : 1280,height:1000});
            const prefix = `${name}-${surface}-${id}`;
            const initialGeneration = await frame.evaluate(()=>window.__noonExampleGallery.generationDiagnostics.runGeneration);
            assert.equal(initialGeneration,1,'exactly one canonical automatic initial Run');
            assert.equal(await frame.evaluate(()=>window.__noonSitePlayingObserved),true,'the initial scene actually played without a gesture');
            assert.equal(await frame.locator('#auto-run').isChecked(),true);
            assert.equal(await frame.locator('button:visible').count(),2,'no playback/editor toolbar in View');
            const duration = await frame.locator('.playback-scrubber').evaluate(element=>Number(element.max));
            assert.ok(duration>2);
            const samples=[];
            for (const fraction of [.12,.45,.8]) samples.push(await sample(frame,`${prefix}-${fraction}`,fraction*duration));
            assert.ok(new Set(samples.map(item=>item.sha256)).size>=2,'authored times show different real frames');
            const geometry=await frame.locator('#scene').boundingBox(),bounds=await element.boundingBox();
            assert.ok(geometry && bounds && geometry.y>=bounds.y && geometry.y+geometry.height<=bounds.y+bounds.height+1,'canvas visible without nested scrolling');
            // Keyboard pause/play/restart use the canonical controller without a toolbar.
            await frame.locator('.preview-pane').focus();
            await frame.locator('.preview-pane').press('Space');
            await frame.waitForFunction(()=>document.querySelector('#status').dataset.playbackPlaying==='true');
            await frame.locator('.preview-pane').press('Space');
            await frame.waitForFunction(()=>document.querySelector('#status').dataset.playbackPlaying==='false');
            await frame.locator('.preview-pane').press('Home');
            await frame.waitForFunction(()=>parseFloat(document.querySelector('#metric-time').value)<1);
            await frame.getByRole('tab',{name:'Code',exact:true}).click();
            await frame.getByRole('tab',{name:'View',exact:true}).click();
            assert.equal(await frame.evaluate(()=>window.__noonExampleGallery.generationDiagnostics.runGeneration),initialGeneration,'flipping View/Code is not source execution');
            await edit(frame,page,editSource('BLUE'));
            await ready(frame,initialGeneration);
            const editedGeneration=await frame.evaluate(()=>window.__noonExampleGallery.generationDiagnostics.runGeneration);
            await frame.getByRole('tab',{name:'View',exact:true}).click();
            const edited=await sample(frame,`${prefix}-hot-edit`,1,1);
            assert.ok(samples.every(item=>item.sha256!==edited.sha256),'editing changed actual canvas output');
            let recovery=null;
            if (index===0) {
              await edit(frame,page,'from noon import *\nclass Broken(:\n');
              await frame.waitForFunction(previous=>window.__noonExampleGallery.generationDiagnostics.runGeneration>previous && document.querySelector('#patch-status').dataset.state==='error',editedGeneration);
              assert.equal(await frame.locator('.editor-footer').isVisible(),true,'error remains visible in Code');
              const failedGeneration=await frame.evaluate(()=>window.__noonExampleGallery.generationDiagnostics.runGeneration);
              await edit(frame,page,editSource('PINK'));
              await ready(frame,failedGeneration);
              await frame.getByRole('tab',{name:'View',exact:true}).click();
              recovery=await sample(frame,`${prefix}-recovered`,1,1);
              assert.notEqual(recovery.sha256,edited.sha256,'fixed code replaces blue pixels with pink pixels');
            }
            assert.equal(await element.evaluate(node=>node.isConnected),true,'hot reload keeps the same iframe/editor');
            assert.equal(await page.locator('iframe').count(),selection.length,'editing one scene does not evict the others');
            report.push({browser:name,surface,id,duration,automaticStart:true,initialGeneration,samples,edited,recovery,workerRetirement:false});
            assert.deepEqual(errors,[],`${prefix}: no uncaught page errors`);
          }
          await page.goto('about:blank');
          await workersRetired(workers);
          for (const row of report.slice(start)) row.workerRetirement=true;
        } catch (error) {
          await page.screenshot({path:path.join(output,`${name}-${surface}-failure.png`)}).catch(()=>{});
          await writeFile(path.join(output,`${name}-${surface}-failure.json`),JSON.stringify({error:String(error.stack??error),errors,report},null,2));
          throw error;
        }
      }
      await context.close();
    } finally { await browser.close(); }
  }
  await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2));
  console.log(`Live site: ${report.length} real browser/surface/example cases passed autoplay, View/Code, hot edits and navigation teardown.`);
} finally { await server.close(); }
