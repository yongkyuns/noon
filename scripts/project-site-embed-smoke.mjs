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
import { captureStalledRenderStacks } from './noon-site-worker-stack.mjs';
import { observeUnpromptedPlayback, observedUnpromptedProgress } from './project-site-autoplay-observer.mjs';

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
// Read-only failure evidence: do not start/retry a Run or query the execution
// transport while diagnosing a stalled one. A wedged/detached frame must not
// prevent the original assertion and the other frames' evidence being retained.
async function captureFrameState(frame) {
  let timer;
  try {
    return await Promise.race([
      frame.evaluate(() => {
        const gallery = window.__noonExampleGallery;
        const read = selector => {
          const node = document.querySelector(selector);
          return node ? {data:{...node.dataset},value:node.value ?? node.textContent,
            disabled:node.disabled,checked:node.checked} : null;
        };
        const source = document.querySelector('#python-scene-source');
        const canvas = document.querySelector('#scene');
        return {
          url:location.href,visibility:document.visibilityState,focused:document.hasFocus(),
          activeElement:document.activeElement?.className,embed:{...document.documentElement.dataset},
          automaticStart:window.__noonSitePlayingObserved,
          workerHostTraces:window.__noonSiteWorkerHostTraces ?? [],
          gallery:gallery ? {selectedExampleId:gallery.selectedExampleId,runInFlight:gallery.runInFlight,
            generations:gallery.generationDiagnostics,runPhaseMetrics:gallery.runPhaseMetrics,
            executionDiagnostics:gallery.executionDiagnostics} : null,
          status:read('#status'),draft:read('#draft-status'),patch:read('#patch-status'),
          autoRun:read('#auto-run'),run:read('#replace-scene'),scrubber:read('.playback-scrubber'),
          playback:read('.playback-controls'),restart:read('.playback-restart'),
          time:read('#metric-time'),traceback:read('#python-traceback'),
          source:source ? {value:source.value,
            nativeValue:Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').get.call(source),
            editorValue:source.editorView?.state.doc.toString()} : null,
          canvas:canvas ? {rect:canvas.getBoundingClientRect().toJSON(),width:canvas.width,height:canvas.height,
            visibility:getComputedStyle(canvas).visibility} : null,
        };
      }),
      new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('frame diagnostics timed out')),5000);}),
    ]);
  } catch (error) {
    return {url:frame.url(),captureError:String(error)};
  } finally { clearTimeout(timer); }
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
  // Firefox can mis-hit-test the middle of a long CodeMirror document in a
  // nested iframe after scrolling. Focus its real editable node and issue real
  // keyboard input; never write the textarea or invoke a synthetic Run.
  await editor.focus();
  await frame.waitForFunction(() => document.activeElement?.matches('.cm-content'));
  await editor.press('ControlOrMeta+A');
  await page.keyboard.insertText(source);
  // Verify the real editor committed the requested text before waiting for its
  // ordinary auto-run. This is observation, not a synthetic input/Run fallback.
  await frame.waitForFunction(expected => {
    const textarea = document.querySelector('#python-scene-source');
    return textarea?.value === expected && textarea.editorView?.state.doc.toString() === expected;
  },source);
}
try {
  for (const [name, engine] of Object.entries({chromium,firefox,webkit})) {
    const browser = await engine.launch(playgroundLaunchOptions(name));
    try {
      const context = await browser.newContext({viewport:{width:1280,height:1000},colorScheme:'dark'});
      const cache = createPyodideResourceCache(await readFile(path.join(root,'_site/python-worker.js'),'utf8'));
      await cache.install(context);
      // Record transition messages at the iframe host, independent of worker
      // responsiveness. Passive only: preserve native Worker construction,
      // postMessage arguments/transfer list and runtime readiness gates.
      await context.addInitScript(() => {
        const NativeWorker = globalThis.Worker;
        if (!NativeWorker?.prototype?.postMessage) return;
        const original = NativeWorker.prototype.postMessage;
        const known = new WeakMap();
        const traces = [];
        const stages = new Set(['init','prepare','start_engine','rebuild_engine','switch_engine',
          'attach_engine','ready','prepared','engine_started','renderer_rebuilt','mode_switched',
          'error','render_error','stop']);
        globalThis.__noonSiteWorkerHostTraces = traces;
        function record(trace, direction, message) {
          if (!stages.has(message?.type)) return;
          trace.events.push({direction,type:message.type,requestId:message.requestId ?? null,
            atMs:performance.now()});
          if (trace.events.length>64) trace.events.shift();
        }
        NativeWorker.prototype.postMessage = function (...args) {
          let trace = known.get(this);
          if (!trace) {
            trace = {id:traces.length+1,events:[]};
            known.set(this,trace);
            traces.push(trace);
            this.addEventListener('message',event=>record(trace,'received',event.data));
          }
          record(trace,'sent',args[0]);
          return original.apply(this,args);
        };
      });
      // Observe genuine automatic playback before the iframe application runs.
      // The fallback poll only reads status and is removed after first playback.
      await context.addInitScript(observeUnpromptedPlayback);
      const page = await context.newPage();
      page.setDefaultTimeout(120000);
      const errors=[], workers=new Set(), consoleMessages=[], requestFailures=[];
      const retain = (items,item) => { items.push(item); if(items.length>40) items.shift(); };
      page.on('console',message=>{
        if(['warning','error'].includes(message.type())) retain(consoleMessages,
          {type:message.type(),text:message.text(),location:message.location()});
      });
      context.on('requestfailed',request=>retain(requestFailures,
        {url:request.url(),failure:request.failure()}));
      page.on('pageerror',error=>errors.push(String(error)));
      page.on('worker',worker=>{
        workers.add(worker);
        worker.on('close',()=>workers.delete(worker));
        // Passive test-only diagnostics: distinguish renderer restart requests
        // from replies when the original unchanged readiness assertion stalls.
        if (worker.url().includes('render-worker.js')) {
          void worker.evaluate(() => {
            const stages = new Set(['init','prepare','start_engine','rebuild_engine',
              'switch_engine','attach_engine','ready','prepared','engine_started',
              'renderer_rebuilt','error','render_error','stop']);
            const events = [];
            self.__noonRenderTransitionTrace = events;
            const record = (direction, message) => {
              if (!stages.has(message?.type)) return;
              events.push({direction,type:message.type,requestId:message.requestId ?? null,
                atMs:performance.now()});
              if (events.length > 64) events.shift();
            };
            self.addEventListener('message',event=>record('received',event.data));
            const original = self.postMessage;
            self.postMessage = function (...args) {
              record('sent',args[0]);
              return original.apply(this,args);
            };
          }).catch(()=>{}); // A closing worker must not change test behavior.
        }
      });
      for (const [surface, route] of [['home','index.html'],['docs','site/docs/examples.html']]) {
        const start = report.length;
        consoleMessages.length=0; requestFailures.length=0;
        let checkpoint={browser:name,surface,stage:'navigation'};
        const mark = (stage,id=null) => {
          checkpoint={browser:name,surface,id,stage};
          console.log(`Live site checkpoint: ${JSON.stringify(checkpoint)}`);
        };
        try {
          await page.goto(`${server.baseUrl}/_site/${route}`);
          await page.waitForFunction(count=>document.querySelectorAll('iframe[data-shell-ready=true]').length===count,selection.length);
          const frames = await Promise.all(selection.map(async ({id}) => {
            const element = await page.locator(`[data-demo="${id}"] iframe`).elementHandle();
            return {id,element,frame:await element.contentFrame()};
          }));
          // No Run, open-preview or Play gesture: every scene must start itself.
          mark('automatic-start');
          await Promise.all(frames.map(({frame})=>ready(frame)));
          assert.ok(workers.size>=selection.length,'all live contexts created runtime workers');
          assert.equal(await page.locator('video, .demo-poster, [data-demo-open]').count(),0);
          for (const [index,{id,element,frame}] of frames.entries()) {
            await page.setViewportSize({width:index%2 ? 390 : 1280,height:1000});
            const prefix = `${name}-${surface}-${id}`;
            mark('authored-frames',id);
            const initialGeneration = await frame.evaluate(()=>window.__noonExampleGallery.generationDiagnostics.runGeneration);
            assert.equal(initialGeneration,1,'exactly one canonical automatic initial Run');
            // A frame may have played thousands of real frames while Firefox
            // missed the injected observer's mutation/poll callbacks. Qualify
            // genuine autoplay directly: no Run/Play gesture, same generation,
            // active playback and both actual frame/time counters advancing.
            await frame.waitForFunction(() => {
              const frames = Number(document.querySelector('#status')?.dataset.presentedFrames);
              const time = Number.parseFloat(document.querySelector('#metric-time')?.value);
              return frames > 0 && Number.isFinite(time);
            }, null, {timeout:30000});
            const autoplayBefore = await frame.evaluate(() => ({
              generation: window.__noonExampleGallery.generationDiagnostics.runGeneration,
              frames: Number(document.querySelector('#status').dataset.presentedFrames),
              time: Number.parseFloat(document.querySelector('#metric-time').value),
            }));
            await frame.waitForFunction(observedUnpromptedProgress, autoplayBefore, {timeout:30000});
            const autoplayAfter = await frame.evaluate(() => ({
              frames: Number(document.querySelector('#status').dataset.presentedFrames),
              time: Number.parseFloat(document.querySelector('#metric-time').value),
            }));
            assert.equal(await frame.locator('#auto-run').isChecked(),true);
            assert.equal(await frame.locator('button:visible').count(),2,'no playback/editor toolbar in View');
            const duration = await frame.locator('.playback-scrubber').evaluate(element=>Number(element.max));
            assert.ok(duration>2);
            const samples=[];
            for (const fraction of [.12,.45,.8]) samples.push(await sample(frame,`${prefix}-${fraction}`,fraction*duration));
            assert.ok(new Set(samples.map(item=>item.sha256)).size>=2,'authored times show different real frames');
            const geometry=await frame.locator('#scene').boundingBox(),bounds=await element.boundingBox();
            assert.ok(geometry && bounds && geometry.y>=bounds.y && geometry.y+geometry.height<=bounds.y+bounds.height+1,'canvas visible without nested scrolling');
            mark('keyboard-and-tabs',id);
            // Keyboard pause/play/restart use the canonical controller without a toolbar.
            await frame.locator('.preview-pane').focus();
            await frame.locator('.preview-pane').press('Space');
            await frame.waitForFunction(()=>document.querySelector('#status').dataset.playbackPlaying==='true');
            await frame.locator('.preview-pane').press('Space');
            await frame.waitForFunction(()=>document.querySelector('#status').dataset.playbackPlaying==='false');
            // The controller temporarily disables restart while a preceding transport
          // transition settles. Wait for its actual command-ready state rather than
          // treating the optimistic paused status as an acknowledgement.
          await frame.waitForFunction(() => document.querySelector('.playback-restart')?.disabled === false);
          await frame.locator('.preview-pane').press('Home');
            await frame.waitForFunction(()=>parseFloat(document.querySelector('#metric-time').value)<1);
            await frame.getByRole('tab',{name:'Code',exact:true}).click();
            await frame.getByRole('tab',{name:'View',exact:true}).click();
            assert.equal(await frame.evaluate(()=>window.__noonExampleGallery.generationDiagnostics.runGeneration),initialGeneration,'flipping View/Code is not source execution');
            mark('hot-edit-input',id);
            await edit(frame,page,editSource('BLUE'));
            mark('hot-edit-ready',id);
            await ready(frame,initialGeneration);
            const editedGeneration=await frame.evaluate(()=>window.__noonExampleGallery.generationDiagnostics.runGeneration);
            await frame.getByRole('tab',{name:'View',exact:true}).click();
            const edited=await sample(frame,`${prefix}-hot-edit`,1,1);
            assert.ok(samples.every(item=>item.sha256!==edited.sha256),'editing changed actual canvas output');
            let recovery=null;
            if (index===0) {
              mark('syntax-error',id);
              await edit(frame,page,'from noon import *\nclass Broken(:\n');
              await frame.waitForFunction(previous=>window.__noonExampleGallery.generationDiagnostics.runGeneration>previous && document.querySelector('#patch-status').dataset.state==='error',editedGeneration);
              assert.equal(await frame.locator('.editor-footer').isVisible(),true,'error remains visible in Code');
              const failedGeneration=await frame.evaluate(()=>window.__noonExampleGallery.generationDiagnostics.runGeneration);
              mark('recovery-input',id);
              await edit(frame,page,editSource('PINK'));
              mark('recovery-ready',id);
              await ready(frame,failedGeneration);
              await frame.getByRole('tab',{name:'View',exact:true}).click();
              recovery=await sample(frame,`${prefix}-recovered`,1,1);
              assert.notEqual(recovery.sha256,edited.sha256,'fixed code replaces blue pixels with pink pixels');
            }
            assert.equal(await element.evaluate(node=>node.isConnected),true,'hot reload keeps the same iframe/editor');
            assert.equal(await page.locator('iframe').count(),selection.length,'editing one scene does not evict the others');
            report.push({browser:name,surface,id,duration,automaticStart:true,autoplayProgress:{before:autoplayBefore,after:autoplayAfter},initialGeneration,samples,edited,recovery,workerRetirement:false});
            assert.deepEqual(errors,[],`${prefix}: no uncaught page errors`);
          }
          mark('navigation-teardown');
          await page.goto('about:blank');
          await workersRetired(workers);
          for (const row of report.slice(start)) row.workerRetirement=true;
        } catch (error) {
          const frames=await Promise.all(page.frames().map(captureFrameState));
          // Query only existing workers. An unresponsive worker gets a bounded
          // diagnostic timeout; neither readiness nor the original error changes.
          const workerTransitionTraces=await Promise.all([...workers].map(async worker=>{
            let timeout;
            try {
              const trace=await Promise.race([
                worker.evaluate(()=>self.__noonRenderTransitionTrace ?? null),
                new Promise((_,reject)=>{timeout=setTimeout(()=>reject(new Error('worker trace timed out')),2000);}),
              ]);
              return {url:worker.url(),trace};
            } catch (error) {
              return {url:worker.url(),traceError:String(error)};
            } finally {clearTimeout(timeout);}
          }));
          const workerDebuggerStacks = name === 'chromium'
            ? await captureStalledRenderStacks(page) : [];
          const failure={error:String(error.stack??error),checkpoint,errors,consoleMessages,requestFailures,
            workers:[...workers].map(worker=>worker.url()),workerTransitionTraces,workerDebuggerStacks,frames,report};
          // Persist state before attempting a screenshot, which may also fail.
          await writeFile(path.join(output,`${name}-${surface}-failure.json`),JSON.stringify(failure,null,2));
          console.error(`Live site failure: ${JSON.stringify(failure)}`);
          await page.screenshot({path:path.join(output,`${name}-${surface}-failure.png`),timeout:5000}).catch(()=>{});
          throw error;
        }
      }
      await context.close();
    } finally { await browser.close(); }
  }
  await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2));
  console.log(`Live site: ${report.length} real browser/surface/example cases passed autoplay, View/Code, hot edits and navigation teardown.`);
} finally { await server.close(); }
