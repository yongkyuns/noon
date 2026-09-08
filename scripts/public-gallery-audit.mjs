import { chromium, webkit, devices } from 'playwright';
import { mkdir, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';

const base = 'https://yongkyuns.github.io/noon/';
const profile = process.env.AUDIT_PROFILE ?? 'iphone';
const shard = Number(process.env.AUDIT_SHARD ?? 0);
const shards = Number(process.env.AUDIT_SHARDS ?? 2);
const root = `public-gallery-audit/${profile}-${shard}`;
await mkdir(root, { recursive: true });
const json = value => JSON.stringify(value, (_, v) => typeof v === 'bigint' ? String(v) : v, 2);
const hash = value => createHash('sha256').update(value).digest('hex');
async function bounded(work, ms, label) {
  let timer;
  try { return await Promise.race([work, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} exceeded ${ms} ms`)), ms); })]); }
  finally { clearTimeout(timer); }
}
async function getJson(path) {
  const response = await fetch(new URL(path, base), { headers: { 'Cache-Control': 'no-cache' }, signal: AbortSignal.timeout(20000) });
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
  return response.json();
}
const revision = () => getJson(`build-info.json?audit=${Date.now()}`);
const initialRevision = await revision();
const manifests = {};
for (const name of ['manim_tutorial_manifest.json', 'manim_compatibility_manifest.json', 'manim_stress_manifest.json']) {
  manifests[name] = await getJson(`python/examples/${name}`);
}
const inventory = Object.entries(manifests).flatMap(([manifest, value]) => value.entries.map(entry => ({ ...entry, manifest })));
const ready = inventory.filter(entry => entry.status === 'ready');
const entries = ready.filter((_, index) => index % shards === shard);
await writeFile(`${root}/inventory.json`, json({ initialRevision, profile, shard, shards, inventory }));
console.log('INVENTORY', json({ initialRevision, total: inventory.length, ready: ready.length, assigned: entries.map(e => e.id), profile, shard }));
const engine = profile === 'iphone' ? webkit : chromium;
const browser = await engine.launch({ headless: true, ...(engine === chromium ? { args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--disable-dev-shm-usage'] } : {}) });
const options = profile === 'iphone' ? devices['iPhone 13'] : profile === 'android' ? devices['Pixel 7'] : { viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 };
const results = [];
async function audit(entry, attempt) {
  const dir = `${root}/${entry.id}-${attempt}`;
  await mkdir(dir, { recursive: true });
  const result = { id: entry.id, title: entry.title, category: entry.category, manifest: entry.manifest, expectedDuration: entry.expected_duration ?? null, expectedObjectCount: entry.expected_object_count ?? null, profile, attempt, browserVersion: browser.version(), startedAt: new Date().toISOString(), pageErrors: [], consoleErrors: [], failedRequests: [], samples: [], visualValidation: 'diagnostic captures only, not full raster parity' };
  const context = await browser.newContext({ ...options });
  const page = await context.newPage();
  page.setDefaultTimeout(8000);
  page.on('pageerror', error => result.pageErrors.push(String(error)));
  page.on('console', message => { if (message.type() === 'error') result.consoleErrors.push(message.text()); });
  page.on('requestfailed', request => result.failedRequests.push({ url: request.url(), failure: request.failure() }));
  const pendingWrites = [];
  page.on('response', response => {
    if (new URL(response.url()).pathname.endsWith('/' + entry.path)) {
      pendingWrites.push(response.body().then(async bytes => { result.sourceSha256 = hash(bytes); await writeFile(`${dir}/source.py`, bytes); }).catch(error => { result.sourceCaptureError = String(error); }));
    }
  });
  const start = Date.now();
  try {
    result.revisionBefore = await revision();
    await bounded((async () => {
      await page.goto(`${base}?example=${encodeURIComponent(entry.id)}`, { waitUntil: 'domcontentloaded', timeout: 25000 });
      await page.waitForFunction(() => window.__noonExampleGallery !== undefined, null, { timeout: 30000 });
      result.loaded = await page.evaluate(() => ({ selected: window.__noonExampleGallery.selectedExampleId, exampleCount: window.__noonExampleGallery.exampleCount, userAgent: navigator.userAgent, jspi: typeof WebAssembly.promising }));
      if (result.loaded.selected !== entry.id) throw new Error(`Selection mismatch: ${result.loaded.selected}`);
      await page.locator('#scene').scrollIntoViewIfNeeded();
      let lastShotTime = -100, shots = 0, metricsPending = false;
      while (Date.now() - start < 75000) {
        const state = await page.evaluate(() => ({
          selected: window.__noonExampleGallery?.selectedExampleId,
          runInFlight: window.__noonExampleGallery?.runInFlight,
          patch: { ...document.querySelector('#patch-status')?.dataset },
          patchText: document.querySelector('#patch-status')?.value,
          status: document.querySelector('#status-text')?.textContent,
          statusData: { ...document.querySelector('#status')?.dataset },
          error: document.querySelector('#error-message')?.textContent,
        }));
        result.state = state;
        if (state.patch.state === 'error') { result.outcome = 'runtime-error'; break; }
        const report = await page.evaluate(async () => {
          const gallery = window.__noonExampleGallery;
          if (!window.__auditMetricsTask) {
            window.__auditMetricsTask = Promise.resolve(gallery.executionMetrics()).then(value => { window.__auditLatestMetrics = value; window.__auditMetricsTask = null; }, error => { window.__auditMetricsError = String(error); window.__auditMetricsTask = null; });
          }
          return { value: window.__auditLatestMetrics ?? null, error: window.__auditMetricsError ?? null };
        });
        if (report.error) result.metricsError = report.error;
        const metrics = report.value?.metrics;
        if (metrics && Number.isFinite(metrics.time)) {
          const sample = { wallMs: Date.now() - start, time: metrics.time, objects: metrics.objectCount, frames: String(metrics.presentedFrames), backend: metrics.backend, renderHost: report.value.renderHost };
          if (result.samples.at(-1)?.time !== sample.time) result.samples.push(sample);
          result.metrics = report.value;
          if (state.runInFlight && metrics.time > 0 && metrics.objectCount > 0 && shots < 2 && metrics.time > lastShotTime + 0.35) {
            lastShotTime = metrics.time;
            await page.locator('#scene').screenshot({ path: `${dir}/during-${shots}.png`, timeout: 4000 }).catch(error => { result.captureError = String(error); });
            shots++;
          }
        }
        if (state.patch.state === 'applied' && !state.runInFlight) {
          result.outcome = 'completed';
          result.metrics = await bounded(page.evaluate(() => window.__noonExampleGallery.executionMetrics()), 5000, 'final metrics');
          break;
        }
        if (!state.runInFlight && !['running', 'applied', 'error'].includes(state.patch.state) && Date.now() - start > 3500 && !result.explicitRun) {
          result.explicitRun = true;
          await page.evaluate(() => { void window.__noonExampleGallery.run(); });
        }
        await page.waitForTimeout(150);
      }
      if (!result.outcome) result.outcome = 'timeout';
      if (result.outcome === 'completed') {
        const metrics = result.metrics?.metrics;
        result.invariantWarnings = [];
        if (!metrics || Number(metrics.presentedFrames) <= 0) result.invariantWarnings.push('no presented frames');
        if (entry.expected_object_count != null && metrics?.objectCount !== entry.expected_object_count) result.invariantWarnings.push(`expected ${entry.expected_object_count} objects; observed ${metrics?.objectCount}`);
      }
    })(), 85000, 'example execution');
  } catch (error) {
    result.outcome ??= /exceeded|Timeout/.test(String(error)) ? 'timeout' : 'test-error';
    result.exception = String(error);
  } finally {
    result.elapsedMs = Date.now() - start;
    await bounded(page.screenshot({ path: `${dir}/final-page.png`, timeout: 4000 }), 5000, 'screenshot').catch(() => {});
    await bounded(page.locator('#scene').screenshot({ path: `${dir}/final-canvas.png`, timeout: 4000 }), 5000, 'canvas screenshot').catch(() => {});
    result.revisionAfter = await revision().catch(error => ({ error: String(error) }));
    await bounded(Promise.allSettled(pendingWrites), 5000, 'source recording').catch(() => {});
    await bounded(context.close(), 7000, 'context close').catch(() => {});
    await writeFile(`${dir}/result.json`, json(result));
    results.push(result);
    console.log('EXAMPLE_RESULT', json({ id: result.id, profile, attempt, outcome: result.outcome, error: result.state?.patchText, pageErrors: result.pageErrors, consoleErrors: result.consoleErrors, exception: result.exception, warnings: result.invariantWarnings, elapsedMs: result.elapsedMs, revision: result.revisionBefore }));
  }
  return result;
}
try {
  let next = 0;
  await Promise.all(Array.from({ length: 2 }, async () => {
    while (next < entries.length) {
      const entry = entries[next++];
      await audit(entry, 1);
    }
  }));
  // Fresh-context confirmation of failures; a capture job succeeding is NOT a demo pass.
  for (const entry of entries.filter(e => results.some(r => r.id === e.id && r.outcome !== 'completed'))) await audit(entry, 2);
} finally {
  await browser.close();
  await writeFile(`${root}/results.json`, json({ initialRevision, finalRevision: await revision(), profile, shard, shards, testedEntries: entries.length, results }));
}
