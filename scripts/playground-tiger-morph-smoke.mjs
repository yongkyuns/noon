import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import playwright from 'playwright';
import { PNG } from 'pngjs';
import { playgroundLaunchOptions } from './playground-browser-support.mjs';
import { createPyodideResourceCache } from './pyodide-resource-cache.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const artifacts = path.join(root, 'browser-smoke-artifacts/tiger-morph');
const port = Number(process.env.NOON_TIGER_SMOKE_PORT ?? 4199);
const base = `http://127.0.0.1:${port}/web/`;
const source = await readFile(path.join(root, 'web/python/examples/manim_compatible_svg_tiger_morph.py'), 'utf8');
const result = { errors: [], frames: [] };
let server, browser, context, page;
function foregroundPixels(image) {
  const background = [...image.data.slice(0, 3)];
  let count = 0;
  for (let i = 0; i < image.data.length; i += 4) {
    if (Math.abs(image.data[i] - background[0]) + Math.abs(image.data[i+1] - background[1]) +
        Math.abs(image.data[i+2] - background[2]) > 36) count++;
  }
  return count;
}
await mkdir(artifacts, { recursive: true });
try {
  server = spawn('python3', ['-m', 'http.server', String(port), '--bind', '127.0.0.1', '--directory', root], { stdio: 'ignore' });
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    ready = await fetch(base).then(response => response.ok).catch(() => false);
    if (ready) break;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.ok(ready, 'gallery server did not start');
  const worker = await fetch(new URL('python-worker.js', base));
  assert.ok(worker.ok);
  const cache = createPyodideResourceCache(await worker.text());
  browser = await playwright.chromium.launch(playgroundLaunchOptions('chromium'));
  context = await browser.newContext({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
  await cache.install(context);
  page = await context.newPage();
  page.on('pageerror', error => result.errors.push(String(error)));
  page.on('console', message => { if (message.type() === 'error') result.errors.push(message.text()); });
  await page.goto(`${base}?example=compatible-indicate-square`, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => window.__noonExampleGallery && !window.__noonExampleGallery.runInFlight &&
    document.querySelector('#patch-status')?.dataset.state === 'applied', null, { timeout: 90000 });
  await page.evaluate(source => {
    document.querySelector('#python-scene-source').value = source;
    window.__tigerDone = false;
    window.__tigerError = null;
    Promise.resolve(window.__noonExampleGallery.run())
      .catch(error => { window.__tigerError = String(error); })
      .finally(() => { window.__tigerDone = true; });
  }, source);
  await page.waitForFunction(() => window.__tigerDone === true, null, { timeout: 120000 });
  const state = await page.evaluate(async () => ({
    done: window.__tigerDone,
    error: window.__tigerError,
    patchState: document.querySelector('#patch-status')?.dataset.state,
    patchText: document.querySelector('#patch-status')?.value,
    report: await window.__noonExampleGallery.executionMetrics(),
  }));
  result.lastState = state;
  assert.equal(state.done, true, 'tiger scene did not complete');
  assert.equal(state.error, null, state.error ?? undefined);
  assert.equal(state.patchState, 'applied', state.patchText);
  assert.equal(state.report?.metrics?.objectCount, 138, 'restored tiger leaf count differs');
  // The gallery Run promise is the synchronization boundary exposed by this host,
  // so an external Playwright poll cannot reliably sample its intermediate epochs.
  // Require that the real gallery renderer actually presented a multi-frame run;
  // exact intermediate geometry/paint is covered by tiger-manim-differential.mjs.
  const presentedFrames = Number(state.report?.metrics?.presentedFrames);
  result.presentedFrames = presentedFrames;
  assert.ok(Number.isFinite(presentedFrames) && presentedFrames >= 20,
    `gallery tiger playback presented only ${presentedFrames} frames`);
  const time = Number(state.report?.metrics?.time);
  assert.ok(Number.isFinite(time) && time >= 4.85, `gallery tiger stopped early at ${time}`);
  const bytes = await page.locator('#scene').screenshot({ timeout: 10000 });
  const image = PNG.sync.read(bytes);
  const finalFrame = { time, phase: 'restored', width: image.width, height: image.height,
    foreground: foregroundPixels(image), sha256: createHash('sha256').update(image.data).digest('hex'),
    file: 'frame-restored.png' };
  result.frames.push(finalFrame);
  await writeFile(path.join(artifacts, finalFrame.file), bytes);
  assert.ok(finalFrame.foreground > 1000, 'restored gallery tiger lost filled presentation');
  assert.deepEqual(result.errors, []);
  result.outcome = 'pass';
} catch (error) {
  result.outcome = 'fail';
  result.failure = String(error);
  throw error;
} finally {
  await writeFile(path.join(artifacts, 'tiger-morph.json'), JSON.stringify(result, (_, value) => typeof value === 'bigint' ? value.toString() : value, 2));
  await page?.screenshot({ path: path.join(artifacts, 'gallery.png'), timeout: 5000 }).catch(() => {});
  await context?.close();
  await browser?.close();
  server?.kill('SIGTERM');
}
