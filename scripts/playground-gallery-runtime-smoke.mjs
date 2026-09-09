import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import playwright from 'playwright';
import { playgroundLaunchOptions } from './playground-browser-support.mjs';
import { createPyodideResourceCache } from './pyodide-resource-cache.mjs';
import { AUTHORING_CHANNEL, AUTHORING_PROTOCOL_VERSION, parseAuthoringResult } from '../web/authoring-client.js';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const browserName = process.env.NOON_PLAYGROUND_BROWSER ?? 'webkit';
const profile = process.env.NOON_PLAYGROUND_PROFILE ?? 'mobile-dpr2';
const port = Number(process.env.NOON_GALLERY_PORT ?? 4197);
const external = process.env.NOON_GALLERY_BASE;
const base = external ?? `http://127.0.0.1:${port}/web/`;
const artifacts = path.resolve(root, process.env.NOON_PLAYGROUND_MATRIX_ARTIFACTS ??
  `browser-smoke-artifacts/gallery/${browserName}-${profile}`);
const stringify = value => JSON.stringify(value, (_, v) => typeof v === 'bigint' ? String(v) : v, 2);
const affected = ['manim-lagged-start-map', 'parity-moving-dots', 'parity-rotation-updater', 'compatible-indicate-square'];
await mkdir(artifacts, { recursive: true });
let server, browser, runtimeCache;
const startedAt = performance.now();
const results = [];
async function json(relative) {
  const response = await fetch(new URL(relative, base), { signal: AbortSignal.timeout(20000), headers: { 'Cache-Control': 'no-cache' } });
  assert.ok(response.ok, `${relative}: HTTP ${response.status}`);
  return response.json();
}
try {
  if (!external) {
    server = spawn('python3', ['-m', 'http.server', String(port), '--bind', '127.0.0.1', '--directory', root], { stdio: 'ignore' });
    let ready = false;
    for (let i = 0; i < 100; i++) {
      ready = await fetch(base).then(r => r.ok).catch(() => false);
      if (ready) break;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert.ok(ready, 'gallery HTTP server did not start');
  }
  const workerResponse = await fetch(new URL('python-worker.js', base), { signal: AbortSignal.timeout(20000) });
  assert.ok(workerResponse.ok, 'gallery worker source is unavailable');
  runtimeCache = createPyodideResourceCache(await workerResponse.text());
  const revision = external ? await json(`build-info.json?t=${Date.now()}`) : null;
  if (process.env.NOON_GALLERY_REVISION) assert.equal(revision?.commit, process.env.NOON_GALLERY_REVISION);
  const entries = [];
  for (const manifest of ['manim_tutorial_manifest.json', 'manim_compatibility_manifest.json', 'manim_stress_manifest.json']) {
    entries.push(...(await json(`python/examples/${manifest}`)).entries.filter(e => e.status === 'ready'));
  }
  assert.equal(new Set(entries.map(e => e.id)).size, entries.length, 'duplicate gallery IDs');
  for (const id of affected) assert.ok(entries.some(e => e.id === id), `${id} is no longer selectable`);
  const engine = playwright[browserName];
  assert.ok(engine, `unknown browser ${browserName}`);
  browser = await engine.launch(playgroundLaunchOptions(browserName));
  const options = profile === 'android' ? playwright.devices['Pixel 7'] : profile.startsWith('mobile') ?
    playwright.devices['iPhone 13'] : { viewport: { width: 1280, height: 900 }, deviceScaleFactor: profile.endsWith('dpr2') ? 2 : 1 };
  const queue = entries.map(entry => ({ entry, noJspi: false }));
  // The affected exact sources must also finish when JSPI is absent altogether.
  // This includes Chromium so a working desktop synchronous path cannot mask a
  // failure to enter the portable path.
  if (browserName !== 'firefox') for (const id of affected) queue.push({ entry: entries.find(e => e.id === id), noJspi: true });
  let next = 0;
  async function check({ entry, noJspi }) {
    const caseStartedAt = performance.now();
    const context = await browser.newContext({ ...options });
    await runtimeCache.install(context);
    // Observe the existing result envelope without guessing its payload shape.
    // The production parser below validates the semantic descriptor and duration;
    // semantic_execution is an object, not the boolean true.
    // Renderer time may precede completion of an authored static wait.
    await context.addInitScript(({ channel, protocolVersion }) => {
      window.__galleryAuthoringResults = [];
      window.Worker = new Proxy(window.Worker, {
        construct(target, args, newTarget) {
          const worker = Reflect.construct(target, args, newTarget);
          worker.addEventListener('message', ({ data }) => {
            if (data?.channel === channel && data.type === 'result') {
              if (data.protocolVersion !== protocolVersion) {
                window.__galleryAuthoringCaptureError = 'unexpected authoring protocol version';
              } else {
                window.__galleryAuthoringResults.push(data.resultJson);
              }
            }
          });
          return worker;
        },
      });
    }, { channel: AUTHORING_CHANNEL, protocolVersion: AUTHORING_PROTOCOL_VERSION });
    const page = await context.newPage();
    page.setDefaultTimeout(10000);
    const result = { id: entry.id, noJspi, browserName, profile, revision, browserVersion: browser.version(), errors: [], samples: [] };
    const name = `${entry.id}${noJspi ? '-no-jspi' : ''}`;
    if (noJspi) await context.route('**/python-worker.js', async route => {
      const response = await route.fetch();
      await route.fulfill({ response, body: 'delete WebAssembly.promising; delete WebAssembly.Suspending;\n' + await response.text() });
    });
    page.on('pageerror', error => result.errors.push(String(error)));
    page.on('console', msg => { if (msg.type() === 'error') result.errors.push(msg.text()); });
    try {
      await page.goto(`${base}?example=${encodeURIComponent(entry.id)}`, { waitUntil: 'domcontentloaded', timeout: 30000 });
      await page.waitForFunction(() => window.__noonExampleGallery !== undefined, null, { timeout: 45000 });
      let completed = false;
      const deadline = Date.now() + 75000;
      while (Date.now() < deadline) {
        const state = await page.evaluate(() => {
          const gallery = window.__noonExampleGallery;
          if (!window.__galleryMetricsPending) {
            window.__galleryMetricsPending = true;
            Promise.resolve(gallery.executionMetrics()).then(value => { window.__galleryMetrics = value; }, error => { window.__galleryMetricsError = String(error); })
              .finally(() => { window.__galleryMetricsPending = false; });
          }
          return { selected: gallery.selectedExampleId, inFlight: gallery.runInFlight,
            patch: { ...document.querySelector('#patch-status')?.dataset },
            text: document.querySelector('#patch-status')?.value,
            metrics: window.__galleryMetrics ?? null, metricsError: window.__galleryMetricsError };
        });
        result.state = state;
        assert.equal(state.selected, entry.id);
        assert.notEqual(state.patch.state, 'error', state.text);
        assert.equal(state.metricsError, undefined);
        const metric = state.metrics?.metrics;
        if (metric && result.samples.at(-1)?.time !== metric.time) result.samples.push({ time: metric.time, objects: metric.objectCount, frames: metric.presentedFrames });
        if (state.patch.state === 'applied' && !state.inFlight) { completed = true; break; }
        await page.waitForTimeout(100);
      }
      assert.ok(completed, `${entry.id}: initial autoplay did not finish`);
      const metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
      result.finalMetrics = metrics;
      assert.ok(Number(metrics?.metrics?.presentedFrames) > 0, 'no rendered frames');
      const authoring = await page.evaluate(() => ({
        results: window.__galleryAuthoringResults, error: window.__galleryAuthoringCaptureError,
      }));
      assert.equal(authoring.error, undefined);
      assert.equal(authoring.results.length, 1, 'expected exactly one final shared authoring result');
      const parsed = parseAuthoringResult(authoring.results[0]);
      assert.ok(parsed.semanticExecution, 'missing final shared execution descriptor');
      result.semanticExecution = parsed.semanticExecution;
      result.authoredDuration = parsed.duration;
      assert.ok(Number.isFinite(result.authoredDuration), 'missing authored duration');
      if (entry.expected_duration != null) assert.ok(Math.abs(result.authoredDuration - entry.expected_duration) < 1e-6,
        `authored duration ${result.authoredDuration} differs from ${entry.expected_duration}`);
      if (entry.expected_object_count != null) assert.equal(metrics.metrics.objectCount, entry.expected_object_count);
      assert.deepEqual(result.errors, []);
      if (external) assert.equal((await json(`build-info.json?t=${Date.now()}`)).commit, revision.commit, 'public revision changed during gallery validation');
      result.outcome = 'pass';
    } catch (error) {
      result.outcome = 'fail'; result.failure = String(error);
    } finally {
      await page.screenshot({ path: path.join(artifacts, `${name}.png`), timeout: 5000 }).catch(() => {});
      await context.close();
      result.elapsedMs = performance.now() - caseStartedAt;
      results.push(result);
      await writeFile(path.join(artifacts, `${name}.json`), stringify(result));
      console.log(`${result.outcome}: ${name}${result.failure ? `: ${result.failure}` : ''}`);
    }
  }
  await Promise.all(Array.from({ length: 2 }, async () => { while (next < queue.length) await check(queue[next++]); }));
  assert.equal(results.length, queue.length, 'incomplete inventory');
  const failed = results.filter(result => result.outcome !== 'pass');
  assert.deepEqual(failed.map(result => [result.id, result.noJspi, result.failure]), [], 'gallery runtime failures');
  console.log(`All ${entries.length} selectable examples and ${queue.length - entries.length} no-JSPI controls passed.`);
} finally {
  await writeFile(path.join(artifacts, 'results.json'), stringify(results));
  const runtimeResources = { elapsedMs: performance.now() - startedAt, ...runtimeCache?.stats() };
  await writeFile(path.join(artifacts, 'runtime-resources.json'), stringify(runtimeResources));
  console.log('Gallery runtime resources:', stringify(runtimeResources));
  await browser?.close(); server?.kill('SIGTERM');
}
