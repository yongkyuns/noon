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

// Real embedded engine qualification: no stubbed main, workers, assets or canvas.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = path.join(root, 'browser-smoke-artifacts/project-site/embedded');
await mkdir(output, {recursive:true});
const server = await serveRepository(root, 0);
const engines = {chromium, firefox, webkit};
const selection = JSON.parse(await readFile(path.join(root,'web/site/examples.json'),'utf8'));
const report = [];
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
async function workersRetired(workers) {
  const until = Date.now() + 15000;
  while (workers.size && Date.now() < until) await new Promise(resolve=>setTimeout(resolve,25));
  assert.equal(workers.size,0,'closing/removing the iframe must retire its observed workers');
}
function coloredPixels(bytes) {
  const png = pngjs.PNG.sync.read(bytes);
  const buckets = new Set();
  for (let i=0;i<png.data.length;i+=16) {
    const [r,g,b] = png.data.subarray(i,i+3);
    buckets.add(`${r>>4},${g>>4},${b>>4}`);
  }
  assert.ok(png.width>100 && png.height>60,'actual canvas dimensions');
  assert.ok(buckets.size>8,'rendered canvas must not be a flat placeholder');
  return {width:png.width,height:png.height,colorBuckets:buckets.size};
}
try {
  for (const [name, engine] of Object.entries(engines)) {
    const browser = await engine.launch(playgroundLaunchOptions(name));
    try {
      const context = await browser.newContext({viewport:{width:1280,height:1000},colorScheme:'dark'});
      const cache = createPyodideResourceCache(await readFile(path.join(root,'_site/python-worker.js'),'utf8'));
      await cache.install(context);
      const page = await context.newPage();
      page.setDefaultTimeout(120000);
      const errors = [], workers = new Set();
      page.on('pageerror', error=>errors.push(String(error)));
      page.on('worker', worker=>{workers.add(worker);worker.on('close',()=>workers.delete(worker));});
      await page.goto(`${server.baseUrl}/_site/site/docs/examples.html`);
      assert.equal(await page.locator('iframe').count(),0);
      assert.equal(workers.size,0,'documentation reading must not create runtime workers');
      for (const [index, {id}] of selection.entries()) {
        const card = page.locator(`[data-demo="${id}"]`);
        let frame;
        try {
          // Alternate a narrow viewport to qualify the composed host, not just the standalone gallery.
          await page.setViewportSize({width:index%2 ? 390 : 1280,height:1000});
          await card.locator('[data-demo-open]').click();
          await page.waitForFunction(() => document.querySelector('iframe')?.dataset.shellReady === 'true');
          assert.equal(await page.locator('iframe').count(),1);
          frame = await page.locator('iframe').elementHandle().then(handle=>handle.contentFrame());
          await frame.locator('#replace-scene').click();
          await frame.waitForFunction(() => {
            if(document.querySelector('#patch-status')?.dataset.state==='error') throw new Error(document.querySelector('#python-traceback')?.textContent || 'Noon source failed');
            const rail=document.querySelector('.playback-scrubber');
            return document.querySelector('#status')?.dataset.playbackControls==='available' &&
              document.querySelector('#patch-status')?.dataset.state==='applied' && rail && !rail.disabled && Number(rail.max)>2;
          });
          assert.ok(workers.size>0,'real runtime workers must have been observed');
          if (await frame.locator('#status').getAttribute('data-playback-playing') === 'true') {
            await frame.locator('.playback-toggle').click();
          }
          await frame.waitForFunction(()=>document.querySelector('#status').dataset.playbackPlaying==='false');
          const samples = [];
          const duration = await frame.locator('.playback-scrubber').evaluate(element=>Number(element.max));
          for (const fraction of [.12,.45,.8]) {
            const target = fraction*duration;
            await frame.locator('.playback-scrubber').evaluate((element,time)=>{
              element.value=String(time); element.dispatchEvent(new Event('input',{bubbles:true})); element.dispatchEvent(new Event('change',{bubbles:true}));
            },target);
            await frame.waitForFunction(time=>Math.abs(parseFloat(document.querySelector('#metric-time').value)-time)<.2,target);
            const bytes = await frame.locator('#scene').screenshot();
            const pixels = coloredPixels(bytes);
            samples.push({target,time:await frame.locator('#metric-time').evaluate(element=>element.value),sha256:hash(bytes),...pixels});
            await writeFile(path.join(output,`${name}-${id}-${fraction}.png`),bytes);
          }
          assert.ok(new Set(samples.map(sample=>sample.sha256)).size>=2,'different authored times must show different frames');
          await frame.locator('.playback-restart').click();
          await frame.waitForFunction(()=>parseFloat(document.querySelector('#metric-time').value)<1.0);
          const geometry = await frame.locator('#scene').boundingBox();
          const bounds = await page.locator('iframe').boundingBox();
          assert.ok(geometry && bounds && geometry.y>=bounds.y && geometry.y+geometry.height<=bounds.y+bounds.height+1,'preview canvas visible without nested scrolling');
          await card.locator('[data-demo-close]').click();
          await workersRetired(workers);
          assert.equal(await page.locator('iframe').count(),0);
          report.push({browser:name,id,duration,samples,workerRetirement:true});
          assert.deepEqual(errors,[],`${name}/${id}: no page errors`);
        } catch (error) {
          await page.screenshot({path:path.join(output,`${name}-${id}-failure.png`)}).catch(()=>{});
          await writeFile(path.join(output,`${name}-${id}-failure.json`),JSON.stringify({error:String(error.stack??error),errors,report},null,2));
          throw error;
        }
      }
      await context.close();
    } finally { await browser.close(); }
  }
  await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2));
  console.log(`Real embedded preview: ${report.length} browser/example cases passed`);
} finally { await server.close(); }
