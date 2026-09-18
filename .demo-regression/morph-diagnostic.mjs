import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import playwright from 'playwright';
import { playgroundLaunchOptions } from './playground-browser-support.mjs';
const out = 'morph-diagnostics';
await mkdir(out, { recursive: true });
const server = spawn('python3', ['-m', 'http.server', '4199', '--bind', '127.0.0.1'], { stdio: 'ignore' });
const base = 'http://127.0.0.1:4199/web/';
const browser = await playwright.chromium.launch(playgroundLaunchOptions('chromium'));
const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
const errors = [];
const page = await context.newPage();
page.on('pageerror', e => errors.push(e.stack ?? String(e)));
page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
try {
  for (let i=0;i<100;i++) { if (await fetch(base).then(r=>r.ok).catch(()=>false)) break; await new Promise(r=>setTimeout(r,100)); }
  await page.goto(`${base}?example=compatible-svg-tiger-morph`, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  const samples = [];
  for (let i=0;i<130;i++) {
    const state = await page.evaluate(() => ({ wall: performance.now(), text: document.querySelector('#status-text').textContent, patch: document.querySelector('#patch-status').value, phase: document.querySelector('#status').dataset.playbackPhase, time: document.querySelector('.playback-scrubber')?.value, max: document.querySelector('.playback-scrubber')?.max, live: document.querySelector('.playback-controls')?.dataset.controllable }));
    samples.push(state);
    await page.locator('#scene').screenshot({ path: `${out}/ui-${String(i).padStart(3,'0')}.png` });
    await page.waitForTimeout(40);
  }
  await writeFile(`${out}/ui.json`, JSON.stringify(samples, null, 2));
  console.log('UI samples', samples.length, samples.at(-1));
  await page.waitForFunction(() => !window.__noonExampleGallery.runInFlight && document.querySelector('#patch-status').dataset.state === 'applied', null, { timeout: 90000 });
  if (await page.locator('.playback-toggle').textContent() === 'Pause') await page.locator('.playback-toggle').click();
  const times = [0,0.5,1.4,2.25,2.3,2.31,2.7,3.049,3.05,3.06,3.95,4.84,4.85,5.3,5.7];
  for (const time of times) {
    await page.locator('.playback-scrubber').evaluate((node,t) => { node.value=String(t); node.dispatchEvent(new Event('input', {bubbles:true})); }, time);
    await page.waitForFunction(() => document.querySelector('.playback-controls').dataset.busy === 'false');
    await page.locator('#scene').screenshot({path:`${out}/replay-${time}.png`});
  }
  // Separate source-owned sampling uses the existing host boundary, not replay.
  await context.route('**/morph-diagnostic.html', r => r.fulfill({contentType:'text/html', body:'<!doctype html><canvas id="scene" width="704" height="396" style="width:704px;height:396px"></canvas>'}));
  await page.goto(`${base}morph-diagnostic.html`);
  const source = await readFile('web/python/examples/manim_compatible_svg_tiger_morph.py','utf8');
  await page.evaluate(async source => {
    const {PythonAuthoringClient} = await import('./authoring-client.js');
    const {AuthoringExecutionClient} = await import('./authoring-execution-client.js');
    const {SemanticPreviewSession} = await import('./semantic-preview-session.js');
    window.preview = new SemanticPreviewSession({createAuthoringClient:()=>new PythonAuthoringClient(),createExecutionClient:options => window.execution=new AuthoringExecutionClient(document.querySelector('#scene'),options),timeoutMs:60000});
    await window.preview.open(source);
  }, source);
  const states = [];
  for (const time of times) {
    const state = await page.evaluate(async t => ({sample:await window.preview.sample(t),state:await window.execution.state(),debug:await window.execution.debugFrame()}),time);
    states.push(state);
    await page.locator('#scene').screenshot({path:`${out}/source-${time}.png`});
  }
  await writeFile(`${out}/source.json`, JSON.stringify(states,null,2));
  console.log('Source samples',states.length);
} finally {
  await writeFile(`${out}/errors.json`, JSON.stringify(errors,null,2));
  await browser.close(); server.kill();
}
