// Uses the existing production-path raster host; no alternate scene engine.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import playwright from 'playwright';
import { PNG } from 'pngjs';
import { playgroundLaunchOptions } from './playground-browser-support.mjs';
import { createPyodideResourceCache } from './pyodide-resource-cache.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(root, 'browser-smoke-artifacts/tiger-differential');
await mkdir(out, { recursive: true });
const encode = value => JSON.stringify(value, (_, item) => typeof item === 'bigint' ? item.toString() : item, 2);
const port = 4197;
const base = `http://127.0.0.1:${port}/web/`;
const source = await readFile(path.join(root, 'web/python/examples/manim_compatible_svg_tiger_morph.py'), 'utf8');
const samples = [{ label: 'original', time: 0 }];
for (const [direction, start] of [['forward', 0.5], ['return', 3.05]]) {
  for (const alpha of [0, 0.01, 0.25, 0.5, 0.75, 0.99, 1]) {
    samples.push({ label: `${direction}-${alpha.toFixed(2)}`, time: start + 1.8 * alpha });
  }
}
samples.push({ label: 'restored-hold', time: 5.6 });
const times = [...new Set([...Array.from({ length: 172 }, (_, i) => i / 30), ...samples.map(x => x.time)])].sort((a,b) => a-b);
const result = { candidate: process.env.GITHUB_SHA, runtimeReference: JSON.parse(await readFile(path.join(root, 'web/runtime-build-identity.json'), 'utf8')), errors: [], captures: [] };
let server, browser, context, page;
try {
  server = spawn('python3', ['-m', 'http.server', String(port), '--bind', '127.0.0.1', '--directory', root], { stdio: 'ignore' });
  let ready = false;
  for (let i=0; i<100; i++) {
    ready = await fetch(base).then(r => r.ok).catch(() => false);
    if (ready) break;
    await new Promise(r => setTimeout(r, 100));
  }
  assert.ok(ready, 'server did not start');
  const worker = await fetch(new URL('python-worker.js', base));
  assert.ok(worker.ok);
  const cache = createPyodideResourceCache(await worker.text());
  browser = await playwright.chromium.launch(playgroundLaunchOptions('chromium'));
  context = await browser.newContext({ viewport: { width: 1000, height: 580 }, deviceScaleFactor: 1 });
  await cache.install(context);
  page = await context.newPage();
  page.on('pageerror', e => result.errors.push(String(e)));
  page.on('console', m => { if (m.type() === 'error') result.errors.push(m.text()); });
  await page.goto(new URL('manim-raster-host.html', base).href, { waitUntil: 'load' });
  await page.waitForFunction(() => window.noonHostRaster, null, { timeout: 30000 });
  result.loaded = await page.evaluate(source => window.noonHostRaster.load(source, 7), source);
  const pngs = [];
  for (const sample of samples) {
    const index = times.indexOf(sample.time);
    assert.ok(index >= 0);
    const metrics = await page.evaluate(({ index, times }) => window.noonHostRaster.renderThrough(index, times), { index, times });
    assert.equal(metrics.error, null);
    assert.equal(metrics.presented, true);
    assert.ok(Math.abs(metrics.publishedTime - sample.time) < 1e-8, `wrong frame epoch at ${sample.label}`);
    const bytes = await page.locator('#scene').screenshot({ timeout: 20000 });
    await writeFile(path.join(out, `${sample.label}.png`), bytes);
    const png = PNG.sync.read(bytes);
    pngs.push(png);
    const debug = await page.evaluate(() => window.noonHostRaster.debugFrame());
    result.captures.push({ ...sample, metrics, debug });
    await writeFile(path.join(out, 'browser.json'), encode(result));
  }
  const first = pngs[0], last = pngs.at(-1);
  let changed = 0;
  for (let i=0; i<first.data.length; i+=4) {
    if (Math.abs(first.data[i]-last.data[i]) + Math.abs(first.data[i+1]-last.data[i+1]) + Math.abs(first.data[i+2]-last.data[i+2]) > 24) changed++;
  }
  result.restorationChangedPixels = changed;
  await page.evaluate(() => window.noonHostRaster.close());
  await page.close();
  page = await context.newPage();
  await page.goto(new URL('manim-raster-host.html', base).href, { waitUntil: 'load' });
  await page.waitForFunction(() => window.noonHostRaster);
  const counterexample = `from noon import *\nclass PaddingPaint(Scene):\n    def construct(self):\n        source = VGroup(Square(fill_opacity=1, stroke_opacity=0), Square(fill_opacity=0, stroke_opacity=0), Square(fill_opacity=1, stroke_opacity=0))\n        target = VGroup(Square(fill_opacity=1, stroke_opacity=0), Square(fill_opacity=1, stroke_opacity=0))\n        self.add(source)\n        self.play(Transform(source, target), run_time=1, rate_func=linear)\n`;
  await page.evaluate(source => window.noonHostRaster.load(source, 2), counterexample);
  await page.evaluate(() => window.noonHostRaster.renderThrough(1, [0, 0.5]));
  result.paddingPaintMidpoint = await page.evaluate(() => window.noonHostRaster.debugFrame());
  assert.equal(changed, 0, `restoration mismatch at ${changed} pixels`);
  assert.deepEqual(result.errors, []);
  result.outcome = 'pass';
} catch (error) {
  result.outcome = 'fail';
  result.failure = String(error.stack ?? error);
  process.exitCode = 1;
} finally {
  console.log(encode({ outcome: result.outcome, failure: result.failure, captures: result.captures.length, restorationChangedPixels: result.restorationChangedPixels }));
  await writeFile(path.join(out, 'browser.json'), encode(result));
  await page?.screenshot({ path: path.join(out, 'last-page.png'), timeout: 5000 }).catch(() => {});
  await context?.close();
  await browser?.close();
  server?.kill('SIGTERM');
}
