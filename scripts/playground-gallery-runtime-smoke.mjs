import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import playwright from 'playwright';
import { disableAuthoringJspi, playgroundLaunchOptions, collectWebKitCrashReports } from './playground-browser-support.mjs';
import { createPyodideResourceCache } from './pyodide-resource-cache.mjs';
import { serveRepository } from './browser-test-server.mjs';
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
const curatedLessons = ['showcase-spatial-scene', 'showcase-three-d-axes', 'showcase-linear-algebra', 'showcase-camera-follows-path'];
const affected = ['compatible-timed-composition', 'parity-moving-dots', 'parity-rotation-updater', 'compatible-indicate-square', 'showcase-always-redraw', ...curatedLessons];
await mkdir(artifacts, { recursive: true });
let server, browser, runtimeCache;
const startedAt = performance.now();
const results = [];
const nativeFailureDiagnostics = browserName === 'webkit' && process.platform === 'darwin' &&
  process.env.GITHUB_ACTIONS === 'true';
async function webKitProcesses() {
  if (!nativeFailureDiagnostics) return [];
  const exec = promisify(execFile);
  const installation = `${path.dirname(playwright.webkit.executablePath())}${path.sep}`;
  const { stdout } = await exec('/bin/ps', ['-axo', 'pid=,comm='], { timeout: 5000, maxBuffer: 1024 * 1024 });
  return stdout.split('\n').flatMap(line => {
    const match = line.trim().match(/^(\d+)\s+(.+)$/);
    return match?.[2].startsWith(installation) ? [{ pid: match[1], executable: match[2] }] : [];
  });
}
async function captureWebKitFailure(name) {
  if (!nativeFailureDiagnostics) return null;
  // macOS launches WebContent/GPU XPC services under launchd, outside Node's
  // process tree. Restrict evidence to this Playwright installation on CI.
  const exec = promisify(execFile);
  try {
    const processes = (await webKitProcesses()).slice(0, 4);
    return await Promise.all(processes.map(async entry => {
      const file = `${name}-process-${entry.pid}.txt`;
      try {
        await exec('/usr/bin/sample', [entry.pid, '2', '10', '-file', path.join(artifacts, file)],
          { timeout: 10000, maxBuffer: 64 * 1024 });
        return { ...entry, file };
      } catch (error) {
        return { ...entry, file, error: String(error).slice(0, 500) };
      }
    }));
  } catch (error) {
    return { error: String(error).slice(0, 500) };
  }
}
async function json(relative) {
  const response = await fetch(new URL(relative, base), { signal: AbortSignal.timeout(20000), headers: { 'Cache-Control': 'no-cache' } });
  assert.ok(response.ok, `${relative}: HTTP ${response.status}`);
  return response.json();
}
try {
  if (!external) {
    server = process.env.NOON_GALLERY_COI === '1'
      ? await serveRepository(root, port, { crossOriginIsolated: true })
      : spawn('python3', ['-m', 'http.server', String(port), '--bind', '127.0.0.1', '--directory', root], { stdio: 'ignore' });
    let ready = false;
    for (let i = 0; i < 100; i++) {
      ready = await fetch(new URL('index.html', base)).then(r => r.ok).catch(() => false);
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
  // Curated performance scenes need the same mobile/WebKit lifecycle coverage.
  const showcase = await json('python/examples/noon_showcase_manifest.json');
  entries.push(...showcase.entries.filter(entry => entry.performance ||
    entry.id === 'showcase-always-redraw' || curatedLessons.includes(entry.id))
    .map(entry => ({ ...entry, expected_duration: entry.duration })));
  assert.equal(new Set(entries.map(e => e.id)).size, entries.length, 'duplicate gallery IDs');
  for (const id of affected) assert.ok(entries.some(e => e.id === id), `${id} is no longer selectable`);
  const engine = playwright[browserName];
  assert.ok(engine, `unknown browser ${browserName}`);
  if (browserName !== 'webkit') browser = await engine.launch(playgroundLaunchOptions(browserName));
  const options = profile === 'android' ? playwright.devices['Pixel 7'] : profile.startsWith('mobile') ?
    playwright.devices['iPhone 13'] : { viewport: { width: 1280, height: 900 }, deviceScaleFactor: profile.endsWith('dpr2') ? 2 : 1 };
  const selectedIds = process.env.NOON_GALLERY_CASES?.split(',').map(id => id.trim()).filter(Boolean);
  if (selectedIds) for (const id of selectedIds) {
    assert.ok(entries.some(entry => entry.id === id), `unknown gallery case: ${id}`);
  }
  const selectedEntries = selectedIds ? entries.filter(entry => selectedIds.includes(entry.id)) : entries;
  assert.ok(selectedEntries.length > 0, 'gallery selection is empty');
  const queue = selectedEntries.map(entry => ({ entry, noJspi: false }));
  // Mapped composition and callback examples must also finish without JSPI.
  // The geometry-only composition has a paired Rust example; the upstream
  // LaggedStartMap tutorial requires unsupported Tex and stays blocked.
  // This includes Chromium so a working desktop synchronous path cannot mask a
  // failure to enter the portable path.
  if (browserName !== 'firefox') for (const entry of selectedEntries) {
    if (affected.includes(entry.id)) queue.push({ entry, noJspi: true });
  }
  let next = 0;
  async function runCase(caseBrowser, { entry, noJspi }) {
    const caseStartedAt = performance.now();
    const caseStartedWallMs = Date.now();
    let nativeProcesses = [];
    const context = await caseBrowser.newContext({ ...options });
    await runtimeCache.install(context);
    if (noJspi) await disableAuthoringJspi(context);
    // Observe the existing result envelope without guessing its payload shape.
    // The production parser below validates the semantic descriptor and duration;
    // semantic_execution is an object, not the boolean true.
    // Renderer time may precede completion of an authored static wait.
    await context.addInitScript(({ channel, protocolVersion }) => {
      window.__galleryAuthoringResults = [];
      window.__galleryWorkerMessages = [];
      window.Worker = new Proxy(window.Worker, {
        construct(target, args, newTarget) {
          const workerArgs = [...args];
          const workerScriptUrl = new URL(workerArgs[0], window.location.href).href;
          const worker = Reflect.construct(target, workerArgs, newTarget);
          worker.addEventListener('message', ({ data }) => {
            window.__galleryWorkerMessages.push({
              workerScriptUrl: workerScriptUrl.slice(0, 500),
              channel: typeof data?.channel === "string" ? data.channel.slice(0, 100) : null,
              type: typeof data?.type === "string" ? data.type.slice(0, 100) : null,
              time: performance.now(),
            });
            if (window.__galleryWorkerMessages.length > 20) window.__galleryWorkerMessages.shift();
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
    const result = { id: entry.id, noJspi, browserName, profile, revision, browserVersion: caseBrowser.version(), errors: [], failedRequests: [], samples: [] };
    const name = `${entry.id}${noJspi ? '-no-jspi' : ''}`;
    const pendingRequests = new Map();
    context.on('request', request => pendingRequests.set(request, {
      url: request.url(), resourceType: request.resourceType(), startedAt: performance.now(),
    }));
    context.on('requestfinished', request => pendingRequests.delete(request));
    context.on('requestfailed', request => {
      result.failedRequests.push({
        url: request.url(), resourceType: request.resourceType(), error: request.failure()?.errorText,
      });
      pendingRequests.delete(request);
    });
    page.on('pageerror', error => result.errors.push(error.stack || String(error)));
    page.on('console', msg => {
      if (msg.type() === 'error') {
        result.errors.push(msg.text());
      }
    });
    // A browser call can stall before the polling loop checks its own deadline.
    // Bound the whole case from Node so the job still publishes its diagnostics.
    let caseTimer;
    let lastWorkerMessages = [];
    try {
      await Promise.race([
        (async () => {
          await page.goto(new URL(`index.html?example=${encodeURIComponent(entry.id)}`, base).href,
            { waitUntil: 'domcontentloaded', timeout: 30000 });
          // Remember the content process while alive; post-crash sampling cannot
          // find it. This Node-side observation does not query scene/runtime state.
          try { nativeProcesses = await webKitProcesses(); } catch { /* Evidence unavailable. */ }
          await page.waitForFunction(() => window.__noonExampleGallery !== undefined, null, { timeout: 45000 });
          let completed = false;
          // Longer authored examples can run materially slower than real time in Firefox CI.
          // Keep wall-clock headroom for a progressing autoplay while still detecting a hang.
          const deadline = Date.now() + 105000;
          while (Date.now() < deadline) {
            const { workerMessages, ...state } = await page.evaluate(() => {
              const gallery = window.__noonExampleGallery;
              if (!window.__galleryMetricsPending) {
                window.__galleryMetricsPending = true;
                Promise.resolve(gallery.executionMetrics()).then(value => { window.__galleryMetrics = value; }, error => { window.__galleryMetricsError = String(error); })
                  .finally(() => { window.__galleryMetricsPending = false; });
              }
              return { workerMessages: window.__galleryWorkerMessages ?? [],
                selected: gallery.selectedExampleId, inFlight: gallery.runInFlight,
                transportMode: gallery.transportMode,
                crossOriginIsolated: window.crossOriginIsolated,
                patch: { ...document.querySelector('#patch-status')?.dataset },
                text: document.querySelector('#patch-status')?.value,
                runtimeStatus: document.querySelector('#status-text')?.textContent,
                rendererBackend: document.querySelector('#status')?.dataset.rendererBackend,
                renderHost: document.querySelector('#status')?.dataset.renderHost,
                metrics: window.__galleryMetrics ?? null, metricsError: window.__galleryMetricsError };
            });
            lastWorkerMessages = workerMessages;
            result.state = state;
            assert.equal(state.selected, entry.id);
            assert.notEqual(state.patch.state, 'error', `${state.text}: ${state.runtimeStatus}`);
            assert.equal(state.metricsError, undefined);
            const metric = state.metrics?.metrics;
            if (metric && result.samples.at(-1)?.time !== metric.time) result.samples.push({
              time: metric.time, objects: metric.objectCount, frames: metric.presentedFrames,
              ...(entry.id === 'showcase-camera-follows-path' ? {
                intervalMs: metric.presentationIntervalMs,
                intervalSamples: metric.presentationIntervalSamples,
              } : {}),
            });
            if (state.patch.state === 'applied' && !state.inFlight) { completed = true; break; }
            await page.waitForTimeout(100);
          }
          assert.ok(completed, `${entry.id}: initial autoplay did not finish`);
          if (process.env.NOON_GALLERY_COI === '1') {
            assert.equal(result.state.crossOriginIsolated, true, 'gallery COI test did not isolate the browser');
            assert.equal(result.state.transportMode, 'shared', 'gallery COI test did not use the shared mailbox');
          }
          if (noJspi) {
            assert.equal(await page.evaluate(() => window.__noonNoJspiWorkerWrapped), true,
              'no-JSPI smoke did not wrap the production authoring worker');
          }
          if (entry.id === 'showcase-camera-follows-path') {
            // Normal live playback, including no-JSPI WebKit, must expose
            // bounded actual submission gaps during the following segment.
            // This is a correctness check, not a hardware cadence budget.
            const following = result.samples.filter(sample => sample.time > 3.8 &&
              sample.time < 6.8 && sample.intervalMs !== null);
            assert.ok(following.length > 0, 'camera follow did not report live frame gaps');
            for (const sample of following) {
              assert.ok(Number.isSafeInteger(sample.intervalSamples) &&
                sample.intervalSamples > 0 && sample.intervalSamples <= 120);
              const interval = sample.intervalMs;
              assert.ok([interval.min, interval.p50, interval.p95, interval.p99, interval.max, interval.mean]
                .every(value => Number.isFinite(value) && value >= 0));
              assert.ok(interval.min <= interval.p50 && interval.p50 <= interval.p95 &&
                interval.p95 <= interval.p99 && interval.p99 <= interval.max);
            }
            // Exercise DOM normalization through the real Rust worker. Observe
            // every delivery so a late recoverable input error cannot pass as
            // a successful autoplay. These are injected events, not hardware.
            result.pointerProbe = await page.evaluate(async () => {
              const { ExecutionWorkerClient } = await import('./execution-worker-client.js');
              const original = ExecutionWorkerClient.prototype.submitBrowserPointerInput;
              const inputs = [], deliveries = [];
              ExecutionWorkerClient.prototype.submitBrowserPointerInput = function (input) {
                inputs.push(input);
                const delivery = original.call(this, input);
                deliveries.push(delivery.then(() => null, error => String(error)));
                return delivery;
              };
              try {
                const canvas = document.querySelector('#scene'), rect = canvas.getBoundingClientRect();
                const emit = (type, button, buttons) => canvas.dispatchEvent(new PointerEvent(type, {
                  pointerId: 73, pointerType: 'mouse', isPrimary: true, button, buttons,
                  clientX: rect.left + rect.width * .2, clientY: rect.top + rect.height * .2,
                }));
                emit('pointerdown', 0, 0); // Unmatched first edge establishes no source.
                emit('pointermove', 0, 0); // Unchanged label is ordinary hover.
                emit('pointerdown', 0, 1);
                emit('pointermove', 0, 1); // Unchanged held label is ordinary motion.
                emit('pointerup', 0, 0);
                let count;
                do {
                  count = deliveries.length;
                  await Promise.all(deliveries);
                  await new Promise(resolve => setTimeout(resolve, 0));
                } while (deliveries.length !== count);
                return { stimulus: 'injected-pointer-events', inputs,
                  errors: (await Promise.all(deliveries)).filter(Boolean),
                  state: document.querySelector('#patch-status').dataset.state };
              } finally { ExecutionWorkerClient.prototype.submitBrowserPointerInput = original; }
            });
            assert.deepEqual(result.pointerProbe.inputs.map(input => input.kind), ['move', 'press', 'move', 'release']);
            assert.equal(new Set(result.pointerProbe.inputs.map(input => input.source_id)).size, 1);
            assert.deepEqual(result.pointerProbe.errors, []);
            assert.equal(result.pointerProbe.state, 'applied');
          }
          const metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
          result.finalMetrics = metrics;
          assert.ok(Number(metrics?.metrics?.presentedFrames) > 0, 'no rendered frames');
          if (entry.id === 'showcase-camera-follows-path') {
            assert.equal(metrics.metrics.presentationIntervalMs, null,
              'completed camera source must not report its static endpoint as a hitch');
            assert.equal(metrics.metrics.presentationIntervalSamples, 0);
          }
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
        })(),
        new Promise((_, reject) => {
          caseTimer = setTimeout(() => reject(new Error(`${name}: browser case exceeded 150 seconds`)), 150000);
        }),
      ]);
      result.outcome = 'pass';
    } catch (error) {
      result.outcome = 'fail'; result.failure = String(error);
      result.failureDiagnostics = {
        pendingRequestCount: pendingRequests.size,
        pendingRequests: [...pendingRequests.values()].slice(0, 20).map(request => ({
          url: request.url.slice(0, 500),
          resourceType: request.resourceType,
          elapsedMs: performance.now() - request.startedAt,
        })),
        workerMessages: lastWorkerMessages,
      };
      // Capture outside the browser protocol: even page.evaluate can be stuck.
      // Failure remains a failure regardless of whether stack collection works.
      result.failureDiagnostics.nativeSamples = await captureWebKitFailure(name);
      if (nativeFailureDiagnostics) {
        result.failureDiagnostics.nativeCrashReports = await collectWebKitCrashReports({
          directories: [path.join(homedir(), 'Library/Logs/DiagnosticReports'), '/Library/Logs/DiagnosticReports'],
          pids: nativeProcesses.map(entry => Number(entry.pid)), startedAtMs: caseStartedWallMs,
          endedAtMs: Date.now(), artifacts, prefix: name,
        });
      }
    } finally {
      clearTimeout(caseTimer);
      await page.screenshot({ path: path.join(artifacts, `${name}.png`), timeout: 5000 }).catch(() => {});
      result.elapsedMs = performance.now() - caseStartedAt;
      results.push(result);
      await writeFile(path.join(artifacts, `${name}.json`), stringify(result));
      console.log(`${result.outcome}: ${name}${result.failure ? `: ${result.failure}` : ''}`);
      if (result.outcome === 'fail') console.error(stringify({ state: result.state, errors: result.errors, failedRequests: result.failedRequests, failureDiagnostics: result.failureDiagnostics }));
      await context.close();
    }
  }
  async function check(spec) {
    if (browserName !== 'webkit') return runCase(browser, spec);
    // Independent examples must not inherit GPU-process state from closed cases.
    // The public workflow separately covers repeated playback in one live page.
    let caseBrowser = null;
    try {
      caseBrowser = await engine.launch(playgroundLaunchOptions(browserName));
      await runCase(caseBrowser, spec);
    } finally {
      await caseBrowser?.close();
    }
  }
  // Keep WebKit's cold WASM compilation and GPU contexts sequential on CI.
  // Cold startup itself is covered by the public playground workflow.
  const concurrency = browserName === 'webkit' ? 1 : 2;
  await Promise.all(Array.from({ length: concurrency }, async () => { while (next < queue.length) await check(queue[next++]); }));
  assert.equal(results.length, queue.length, 'incomplete inventory');
  const failed = results.filter(result => result.outcome !== 'pass');
  assert.deepEqual(failed.map(result => [result.id, result.noJspi, result.failure]), [], 'gallery runtime failures');
  console.log(`All ${selectedEntries.length} ${selectedIds ? 'selected' : 'selectable'} examples and ${queue.length - selectedEntries.length} no-JSPI controls passed.`);
} finally {
  await writeFile(path.join(artifacts, 'results.json'), stringify(results));
  const runtimeResources = { elapsedMs: performance.now() - startedAt, ...runtimeCache?.stats() };
  await writeFile(path.join(artifacts, 'runtime-resources.json'), stringify(runtimeResources));
  console.log('Gallery runtime resources:', stringify(runtimeResources));
  await browser?.close();
  if (typeof server?.close === 'function') await server.close();
  else server?.kill('SIGTERM');
}
