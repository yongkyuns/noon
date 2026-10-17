// CUDA operation -> actual Noon endpoint queries -> actual canvas pixels.
// Uses the same source-owned continuation and renderer as the Playground.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdir, readFile, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import playwright from 'playwright';
import {PNG} from 'pngjs';
import {serveRepository} from './browser-test-server.mjs';
import {browserArgs} from './manim-raster-support.mjs';
import {checkSample, checkHighlight, checkTextRegions} from './cuda-transpose-pixels.mjs';
import {stringifyEvidence} from './cuda-transpose-report.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const artifactDir = path.resolve(process.env.NOON_CUDA_ARTIFACTS ?? path.join(root, 'cuda-transpose-qualification'));
const sourcePath = 'web/python/examples/cuda_transpose_banks.py';
const source = await readFile(path.join(root, sourcePath), 'utf8');
const probe = await readFile(path.join(root, 'tools/cuda-transpose/visual_probe.py'), 'utf8');
const sha256 = text => createHash('sha256').update(text).digest('hex');
const report = {schema: 1, status: 'running', sceneSha256: sha256(source),
  probeSha256: sha256(probe), oracle: 'independent-cpu-closed-form', cudaValidated: false,
  // Existing source-owned continuations allow monotonic sampling, not rewind.
  seekQualification: 'not supported by the source-owned continuation control API; not claimed', cases: []};
await mkdir(artifactDir, {recursive: true});
let capture = null;
if (process.env.NOON_CUDA_CAPTURE) {
  const filename = path.resolve(process.env.NOON_CUDA_CAPTURE);
  const validation = spawnSync('python3', [path.join(root, 'tools/cuda-transpose/capture_cuda.py'),
    '--verify', filename], {encoding:'utf8'});
  assert.ifError(validation.error);
  assert.equal(validation.status, 0, validation.stdout + validation.stderr);
  capture = JSON.parse(await readFile(filename, 'utf8'));
  report.oracle = 'cuda-instrumented-accesses-and-original-kernel-output';
  report.cudaValidated = true;
  report.captureSha256 = sha256(await readFile(filename));
  report.cudaProvenance = capture.provenance;
} else if (process.env.NOON_CUDA_REQUIRE_GPU === '1') {
  throw new Error('GPU qualification requires NOON_CUDA_CAPTURE; a CPU fixture is not GPU evidence');
}

// Mutations are applied to real scene code/objects, NOT to expected reports.
const mutations = {
  none: '',
  wrong_bank: `_original_bank_point = bank_point\ndef bank_point(bank):\n    return _original_bank_point((bank + 1) % 32)\n`,
  wrong_highlight: `_original_focus = CudaTransposeBanks.focus\ndef _bad_focus(self, step):\n    return _original_focus(self, 'store' if step == 'read' else step)\nCudaTransposeBanks.focus = _bad_focus\n`,
  detached_label: `_original_packet_motion = packet_motion\ndef packet_motion(parts, destination, scale=1.0):\n    return _original_packet_motion(parts[:1], destination, scale)\n`,
  invisible_packet: `_original_packet = packet\ndef packet(lane, point, scale=1.0):\n    parts = _original_packet(lane, point, scale)\n    if lane == 9:\n        parts[0].set_fill(opacity=0.0)\n    return parts\n`,
  early_store: `def _early_store(self, trace, requests):
    self.move_step('store', *(a for i, parts in enumerate(requests)
        for a in packet_motion(parts, output_point(i, COLUMN), scale=0.20)),
        seconds=0.5, proof={'kind':'store', 'trace':trace, 'packets':requests})
CudaTransposeBanks.bank_replay = _early_store
`,
  dim_highlight: 'CODE_HIGHLIGHT_FILL = 0.01\n',
};
const expectedFailure = {wrong_bank: /CUDA_FIDELITY\[position\]|CUDA_PIXELS\[packet\]/,
  wrong_highlight: /CUDA_FIDELITY\[highlight\]/,
  detached_label: /CUDA_FIDELITY\[label\]|CUDA_PIXELS\[label\]/,
  invisible_packet: /CUDA_FIDELITY\[visibility\]/,
  early_store: /CUDA_FIDELITY\[ordering\]/,
  dim_highlight: /CUDA_PIXELS\[highlight\]/};
const cases = [
  {column:3, speed:1, fault:'none'},
  {column:0, speed:2, fault:'none'},
  {column:31, speed:0.5, fault:'none'},
  ...Object.keys(expectedFailure).map(fault => ({column:3, speed:1, fault})),
];
function assembled(config) {
  const gpu = capture ? capture.columns[String(config.column)] : null;
  return `${source}\n${probe}\nimport json as _cuda_json\nfrom js import console as _cuda_console\n` +
    `_cuda_config = _cuda_json.loads(${JSON.stringify(JSON.stringify(config))})\n` +
    `SPEED = _cuda_config['speed']\nCOLUMN = _cuda_config['column']\n` +
    `GPU_TRACE = _cuda_json.loads(${JSON.stringify(JSON.stringify(gpu))})\n` +
    mutations[config.fault] +
    `_cuda_probe = CudaVisualProbe(_cuda_config, lambda message: _cuda_console.log('NOON_CUDA_AUDIT ' + _cuda_json.dumps(message, allow_nan=False)))\n` +
    `result = CudaTransposeBanks()\nresult._cuda_probe = _cuda_probe\nresult.construct()\n_cuda_probe.finish(result)\n`;
}
function eventQueue() {
  const events = []; let pending = null;
  return {push(event) { if (pending) { const resolve = pending; pending = null; resolve(event); } else events.push(event); },
    next() { if (events.length) return Promise.resolve(events.shift());
      return new Promise((resolve, reject) => {
        const timeout = setTimeout(() => { pending=null; reject(new Error('No source interval/terminal event within 45 seconds')); }, 45000);
        pending = event => { clearTimeout(timeout); resolve(event); };
      }); },
    drain() { return events.splice(0); }};
}
let browser, server;
try {
  server = await serveRepository(root, Number(process.env.NOON_CUDA_PORT ?? 4228), {crossOriginIsolated:true});
  browser = await playwright.chromium.launch({channel:'chromium', headless:true, args:browserArgs('webgpu')});
  for (const config of cases) {
    const name = `column-${config.column}-speed-${config.speed}-${config.fault}`;
    const dir = path.join(artifactDir, name); await mkdir(dir, {recursive:true});
    const evidence = {name, ...config, status:'running', intervals:0, semanticObservations:0, pixelSamples:[], checkpoints:[]};
    const queue = eventQueue(); const observations = [];
    const page = await browser.newPage({viewport:{width:1280,height:720}, deviceScaleFactor:1});
    let failure = null;
    page.on('console', message => {
      const text = message.text();
      if (text.startsWith('NOON_CUDA_AUDIT ')) {
        try { const value = JSON.parse(text.slice('NOON_CUDA_AUDIT '.length)); observations.push(value); queue.push(value); }
        catch (error) { queue.push({type:'failure', error:String(error)}); }
      }
    });
    page.on('pageerror', error => queue.push({type:'failure', error:String(error)}));
    try {
      await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
      await page.exposeFunction('__cudaFailure', error => queue.push({type:'failure', error}));
      await page.evaluate(async code => {
        const {PythonAuthoringClient} = await import('./authoring-client.js');
        const {AuthoringExecutionClient} = await import('./authoring-execution-client.js');
        document.body.style.margin = '0';
        const canvas = document.querySelector('#scene');
        canvas.width=1280; canvas.height=720; canvas.style.width='1280px'; canvas.style.height='720px';
        const authoring = new PythonAuthoringClient();
        let resolve, reject;
        const attached = new Promise((a,b) => {resolve=a;reject=b;});
        const execution = new AuthoringExecutionClient(canvas, {onError(error, owner) {
          const text = `${owner}: ${error}`; reject(new Error(text)); void window.__cudaFailure(text);
        }});
        const authored = authoring.run(code, {}, {async onSemanticContinuation(registration) {
          try { await execution.startSemanticExecution(registration.semanticExecution, {
            authoringClient:authoring, loopDurationSeconds:Math.max(1, registration.duration),
            transportMode:'transferable', pacing:'external_samples'}); resolve(); }
          catch (error) { reject(error); throw error; }
        }});
        authored.catch(error => {reject(error); void window.__cudaFailure(String(error));});
        window.cudaQualification = {authoring, execution, authored};
        await attached;
      }, assembled(config));
      let completed = false;
      let midpointChecks = 0;
      while (!completed) {
        const event = await queue.next();
        if (event.type === 'failure') throw new Error(event.error);
        if (event.type === 'complete') { evidence.completion = event; completed = true; break; }
        if (event.type === 'observed') { ++evidence.semanticObservations; continue; }
        if (event.type === 'checkpoint') {
          evidence.checkpoints.push(event);
          if (event.highlights || event.text_regions) {
            await page.evaluate(time => window.cudaQualification.execution.sampleToAuthoredTime(time), event.time);
            const bytes = await page.locator('#scene').screenshot();
            await writeFile(path.join(dir, `checkpoint-${event.name}-${event.stride}.png`), bytes);
            const image = PNG.sync.read(bytes);
            if (event.highlights) checkHighlight(image, event.highlights);
            if (event.text_regions) checkTextRegions(image, event.text_regions);
          }
          continue;
        }
        assert.equal(event.type, 'interval'); ++evidence.intervals;
        if (event.kind) {
          for (const [alpha, offset] of [[0.5,0.5], [1,1-1e-6]]) {
            const time = event.start + (event.end-event.start)*offset;
            const retain = !['bank','register'].includes(event.kind) || event.stride === 33 ||
              event.batch.some(lane => [0,9,31].includes(lane));
            const receipt = await page.evaluate(async ({time, retain}) => {
              const h = window.cudaQualification;
              const state = await h.execution.sampleToAuthoredTime(time);
              const metrics = await h.execution.metrics();
              return {state, metrics:metrics.metrics, frame:retain ? await h.execution.debugFrame() : null};
            }, {time, retain});
            assert.ok(Math.abs(receipt.state.time-time) < 0.00002, 'renderer did not acknowledge requested scene time');
            assert.ok(receipt.metrics.presentedFrames > 0 && receipt.metrics.drawCalls > 0, 'empty renderer receipt');
            const filename = `${event.id}-${event.kind}-${alpha===1?'end':'mid'}`;
            const png = await page.locator('#scene').screenshot();
            if (retain) {
              await writeFile(path.join(dir, filename+'.png'), png);
              await writeFile(path.join(dir, filename+'.json'), stringifyEvidence(receipt));
            }
            const checked = checkSample(PNG.sync.read(png), event, alpha);
            midpointChecks += alpha === 0.5 ? checked.checked : 0;
            evidence.pixelSamples.push({id:event.id, kind:event.kind, stride:event.stride, time, alpha, ...checked});
          }
        }
        await page.evaluate(time => window.cudaQualification.execution.sampleToAuthoredTime(time), event.end);
      }
      await page.evaluate(() => window.cudaQualification.authored.then(result => ({duration:result.duration})));
      assert.ok(evidence.semanticObservations >= 76, 'incomplete packet endpoint coverage');
      assert.ok(midpointChecks > 32, 'insufficient unoccluded midpoint coverage');
      assert.deepEqual(evidence.completion.layouts, [32,33]);
      evidence.status = 'passed';
    } catch (error) {
      failure = error;
      evidence.error = String(error?.stack ?? error);
      try { await page.locator('#scene').screenshot({path:path.join(dir, 'failure.png')}); } catch {}
    } finally {
      await writeFile(path.join(dir,'observations.json'), stringifyEvidence(observations));
      await page.evaluate(() => { const h=window.cudaQualification; h?.execution.terminate(); h?.authoring.terminate(); }).catch(()=>{});
      await page.close();
    }
    if (config.fault !== 'none') {
      assert.ok(failure, `${name}: deliberately wrong animation was accepted`);
      assert.match(String(failure), expectedFailure[config.fault], `${name}: wrong failure, not proof of the mutation being detected`);
      evidence.status = 'expected-rejection';
    } else if (failure) {
      evidence.status='failed'; report.cases.push(evidence); throw failure;
    }
    report.cases.push(evidence);
    await writeFile(path.join(artifactDir,'report.json'), stringifyEvidence(report));
    console.log(`${evidence.status}: ${name}`);
  }
  report.status='passed';
} catch (error) {
  report.status='failed'; report.error=String(error?.stack ?? error); throw error;
} finally {
  await browser?.close(); await server?.close();
  await writeFile(path.join(artifactDir,'report.json'), stringifyEvidence(report));
}
