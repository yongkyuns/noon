// Diagnostic only. No runtime/default/gallery changes and no hidden callback fallback.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { chromium } from 'playwright';
import pngjs from 'pngjs';
import { serveRepository } from './browser-test-server.mjs';
import { browserArgs } from './manim-raster-support.mjs';
import { createPyodideResourceCache } from './pyodide-resource-cache.mjs';

const root = process.cwd();
const output = path.join(root, 'browser-smoke-artifacts/gallery-preparation');
await mkdir(output, { recursive: true });
const manifest = JSON.parse(await readFile('web/python/examples/noon_showcase_manifest.json', 'utf8'));
const build = JSON.parse(await readFile('web/build-info.json', 'utf8'));
assert.equal(build.commit, '2a1064cf98839cfc406e9ca59178a41e83c6b505');
const workerSource = await readFile('web/python-worker.js', 'utf8');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const report = {
  schema: 1, build, startedAt: new Date().toISOString(),
  purpose: 'Exploratory preparation latency, not product promotion or physical-device qualification',
  method: {
    source: 'Exact deployed showcase sources; no duration scaling or source edits',
    baseline: 'Production realtime authoring/execution clients, first acknowledged initial frame',
    headless: 'Diagnostic host drives shared Rust logical segments without a renderer, then seals/seeks/renders via production clients',
    unsupported: 'Callbacks and manifest-declared nonreplayable input excluded, never silently skipped',
    setup: 'Worker/runtime initialization measured separately; assets may be job-cache warm. Not a cold-network benchmark.',
    rendering: 'Software WebGL2; same 960x540 canvas and renderer build for both conditions',
    repetitions: 3,
    correctness: 'Three exact-time RGBA comparisons against independent source-driven forward sampling; not complete API/parity qualification',
  },
  environment: { cpus: os.cpus().map(c => c.model), platform: os.platform(), arch: os.arch(), node: process.version },
  workerSha256: hash(workerSource), results: [], samples: [],
};
const save = () => writeFile(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
await save();

// This wrapper changes only the diagnostic host's continuation consumer.
// It uses the existing Rust authored-time driver and completion operation.
// It never acknowledges a fictitious GPU presentation, invokes a fake scene,
// scales clocks, or bypasses replay admission.
const wrapper = `import './python-worker.js';
const send = self.postMessage.bind(self);
let stats = {segments:0, driveMs:0, firstBarrierMs:null};
let runStarted = 0;
const contexts = new WeakSet();
self.addEventListener('message', event => {
  if (event.data?.channel === 'noon.authoring' && event.data?.type === 'run') {
    runStarted = performance.now(); stats = {segments:0, driveMs:0, firstBarrierMs:null};
  }
});
self.postMessage = function(message, ...rest) {
  if (message?.channel === 'noon.authoring' && message.type === 'ready') {
    self.noonAwaitSemanticContinuation = async context => {
      const started = performance.now();
      stats.firstBarrierMs ??= started - runStarted;
      const target = Number(context.liveHandoffDuration());
      if (!Number.isFinite(target) || target < 0) throw new Error('Invalid headless target');
      let player;
      if (!contexts.has(context)) {
        context.prepareExecutionRun();
        player = context.createExecutionPlayer(Math.max(1, target), 0);
        contexts.add(context);
        const initial = player.initialCallbackPhaseJson();
        if (initial != null) throw new Error('HEADLESS_UNSUPPORTED_REQUIRED_CALLBACK');
      } else player = context.resumeExecutionPlayer();
      try {
        const drive = player.driveLiveSegmentToAuthoredTime(target);
        const callback = drive.callbackPhaseJson;
        const reached = drive.reachedEndpoint;
        drive.free();
        if (callback != null) throw new Error('HEADLESS_UNSUPPORTED_REQUIRED_CALLBACK');
        if (!reached) throw new Error('Headless driver did not reach the shared endpoint');
        player.completeLiveSegment();
        context.returnExecutionPlayer(player);
        player = null;
        stats.segments++;
        stats.driveMs += performance.now() - started;
        return JSON.stringify({kind:'complete'});
      } catch (error) {
        // Terminate this diagnostic worker after reporting an error; do not
        // attempt callback recovery or continue with incomplete history.
        throw error;
      }
    };
  }
  if (message?.channel === 'noon.authoring' && message.type === 'result') {
    send({channel:'noon.preparation.diagnostic', stats:{...stats, sourceWallMs:performance.now()-runStarted}});
  }
  return send(message, ...rest);
};
`;
await writeFile('web/gallery-preparation-diagnostic-worker.js', wrapper);
await writeFile('web/gallery-preparation-diagnostic.html', '<!doctype html><meta charset="utf-8"><canvas id="scene" width="960" height="540" style="width:960px;height:540px"></canvas>');
const server = await serveRepository(root, 0);
const cache = createPyodideResourceCache(workerSource);
let browser;

async function execute(entry, mode, trial, checks = false) {
  const source = await readFile(path.join('web', entry.path), 'utf8');
  const context = await browser.newContext({ viewport: {width: 960, height: 540}, deviceScaleFactor: 1 });
  await cache.install(context);
  const page = await context.newPage();
  const sample = { id: entry.id, title: entry.title, mode, trial, sourceSha256: hash(source), errors: [], outcome: 'failed' };
  report.samples.push(sample);
  page.on('pageerror', e => sample.errors.push(String(e)));
  page.setDefaultTimeout(90000);
  try {
    await page.goto(`${server.baseUrl}/web/gallery-preparation-diagnostic.html`);
    const result = await page.evaluate(async ({source, mode, duration}) => {
      const {PythonAuthoringClient} = await import('./authoring-client.js');
      const {AuthoringExecutionClient} = await import('./authoring-execution-client.js');
      const started = performance.now();
      const worker = new Worker(mode === 'headless' ? './gallery-preparation-diagnostic-worker.js' : './python-worker.js', {type:'module'});
      let headlessStats = null;
      // Register before PythonAuthoringClient: its production protocol correctly
      // rejects foreign channels. Consume only this test-only telemetry message;
      // ordinary production messages and their validation remain untouched.
      worker.addEventListener('message', event => {
        if (event.data?.channel !== 'noon.preparation.diagnostic') return;
        event.stopImmediatePropagation();
        headlessStats = event.data.stats;
      });
      const authoring = new PythonAuthoringClient(worker);
      let executionError = null;
      const execution = new AuthoringExecutionClient(document.querySelector('#scene'), {onError: error => { executionError = String(error); }});
      window.__prepDiagnostic = {authoring, execution};
      await Promise.all([authoring.ready(), execution.prepare({transportMode:'transferable'})]);
      const ready = performance.now();
      let resolveAttached, rejectAttached;
      const attached = new Promise((resolve, reject) => {resolveAttached=resolve; rejectAttached=reject;});
      let registration = null, sourceCompleted = false, finalResult = null, sourceMs = null;
      const run = authoring.run(source, {}, {
        onSemanticContinuation: async value => {
          registration = value;
          if (mode === 'headless') throw new Error('Headless wrapper failed to consume continuation');
          try {
            await execution.startSemanticExecution(value.semanticExecution, {authoringClient:authoring, loopDurationSeconds:duration, transportMode:'transferable', pacing:mode==='forward'?'external_samples':'realtime'});
            resolveAttached();
          } catch(error) { rejectAttached(error); throw error; }
        }
      });
      run.then(result => { sourceCompleted=true; finalResult=result; sourceMs=performance.now()-ready; }, rejectAttached);
      run.catch(() => {});
      if (mode === 'headless') {
        const result = await run;
        if (result.semanticExecution?.continuationGeneration != null) throw new Error('Unconsumed headless source');
        await execution.startSemanticExecution(result.semanticExecution, {authoringClient:authoring, loopDurationSeconds:duration, transportMode:'transferable', initiallyPaused:true});
        await execution.seek(0);
      } else await attached;
      const frame = await execution.metrics();
      const firstFrameMs = performance.now() - ready;
      if (executionError) throw new Error(executionError);
      if (!(frame?.metrics?.objectCount > 0)) throw new Error('No scene objects in initial frame');
      window.__prepDiagnostic.run = run;
      window.__prepDiagnostic.sample = async time => {
        const state = mode==='headless' ? await execution.seek(time) : await execution.sampleToAuthoredTime(time);
        if (Math.abs(state.time-time)>1e-7) throw new Error('Requested sample time not reached');
        return {time:state.time, metrics:await execution.metrics()};
      };
      return {setupMs:ready-started, firstFrameMs, totalReadyMs:performance.now()-started, sourceMs, headlessStats, sourceCompleted, rendererBackend:execution.rendererBackend, objectCount:frame.metrics.objectCount, memory:performance.memory ? {usedJSHeapSize:performance.memory.usedJSHeapSize} : null};
    }, {source, mode, duration:entry.duration});
    Object.assign(sample, result);
    assert.equal(result.rendererBackend, 'WebGL2');
    if (mode === 'headless') assert.equal(result.sourceCompleted, true);
    if (checks) {
      sample.frames = [];
      const times = [entry.duration*0.25, entry.duration*0.5, entry.duration*0.75];
      if (mode==='headless') times.reverse();
      for (const time of times) {
        const frame = await page.evaluate(time => window.__prepDiagnostic.sample(time), time);
        const bytes = await page.locator('#scene').screenshot();
        const rgba = pngjs.PNG.sync.read(bytes);
        const filename = `${entry.id}-${mode}-${time.toFixed(4)}.png`;
        await writeFile(path.join(output, filename), bytes);
        sample.frames.push({time, filename, pixelSha256:hash(rgba.data), width:rgba.width, height:rgba.height, objectCount:frame.metrics.metrics.objectCount});
      }
    }
    sample.outcome = 'passed';
  } catch(error) {
    sample.error = String(error.stack ?? error);
  } finally {
    await page.evaluate(() => { window.__prepDiagnostic?.execution.terminate(); window.__prepDiagnostic?.authoring.terminate(); }).catch(()=>{});
    await context.close();
    await save();
  }
  console.log(JSON.stringify({id:entry.id, mode, trial, outcome:sample.outcome, setupMs:sample.setupMs, firstFrameMs:sample.firstFrameMs, sourceMs:sample.sourceMs, headlessStats:sample.headlessStats, error:sample.error}));
  return sample;
}

try {
  browser = await chromium.launch({headless:true, args:browserArgs('webgl')});
  report.environment.browser = browser.version();
  const selection = process.env.NOON_PREPARATION_CASES?.split(',').filter(Boolean);
  for (const entry of manifest.entries) {
    const result = {id:entry.id, title:entry.title, duration:entry.duration, capability:entry.playback_capability ?? 'deterministic-retained-replay'};
    report.results.push(result);
    if (selection && !selection.includes(entry.id)) { result.outcome='not_selected'; continue; }
    if (result.capability !== 'deterministic-retained-replay') { result.outcome='unsupported_by_current_replay'; result.limitation=entry.playback_limitation; continue; }
    const baseline = [], candidates = [];
    for (let trial=0; trial<3; trial++) {
      const modes = trial%2 ? ['headless','baseline'] : ['baseline','headless'];
      for (const mode of modes) {
        const sample = await execute(entry,mode,trial,mode==='headless'&&trial===0);
        (mode==='headless'?candidates:baseline).push(sample);
      }
      if (candidates.at(-1)?.outcome!=='passed') break;
    }
    if (candidates[0]?.outcome==='passed') {
      const forward = await execute(entry,'forward',0,true);
      result.correctness = forward.outcome==='passed' && forward.frames.every(frame => candidates[0].frames.some(other => other.time===frame.time && other.pixelSha256===frame.pixelSha256));
    } else result.correctness=false;
    const passed = baseline.length===3 && candidates.length===3 && [...baseline,...candidates].every(s=>s.outcome==='passed') && result.correctness;
    result.outcome = passed?'qualified_exploratory_samples':'failed_or_incomplete';
    if (passed) {
      const median = xs => [...xs].sort((a,b)=>a-b)[Math.floor(xs.length/2)];
      result.baselineFirstFrameMs = median(baseline.map(s=>s.firstFrameMs));
      result.preparedFirstFrameMs = median(candidates.map(s=>s.firstFrameMs));
      result.preparationMs = median(candidates.map(s=>s.sourceMs));
      result.addedWaitMs = result.preparedFirstFrameMs-result.baselineFirstFrameMs;
    }
    await save();
  }
} finally {
  report.resourceCache = cache.stats();
  report.finishedAt = new Date().toISOString();
  await save();
  await browser?.close();
  await server.close();
}
console.log('SUMMARY '+JSON.stringify(report.results));
if (report.results.some(r=>r.outcome==='failed_or_incomplete')) process.exitCode=1;
