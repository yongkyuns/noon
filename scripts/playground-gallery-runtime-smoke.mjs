import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import playwright from 'playwright';
import { disableAuthoringJspi, playgroundLaunchOptions, collectWebKitCrashReports, waitForBrowserObservation } from './playground-browser-support.mjs';
import { captureAttemptedPage, createBrowserFailureTrace, createNativeWebKitEvidence } from './playground-browser-failure-evidence.mjs';
import { createPyodideResourceCache } from './pyodide-resource-cache.mjs';
import { serveRepository } from './browser-test-server.mjs';
import { seekPausedGallery, waitForPublishedGalleryFrame } from './showcase-playback.mjs';
import { AUTHORING_CHANNEL, AUTHORING_PROTOCOL_VERSION, parseAuthoringResult } from '../web/authoring-client.js';
import { galleryCaseQueue, galleryEntriesFromManifests, galleryManifestPaths, galleryPoolForProfileGate, galleryShardFromEnvironment, shardGalleryEntries } from './gallery-shards.mjs';
import { assertGalleryRuntimeBuildIdentityUnchanged, evaluateShowcaseDiagnostics, galleryCaseLifecycleReady, showcaseAudioGestureReady, SHOWCASE_PROFILE_EXPECTATIONS, validateGalleryRuntimeBuildIdentity, validateShowcaseInventory, validateShowcaseDiagnostics } from './playground-profile-expectations.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const browserName = process.env.NOON_PLAYGROUND_BROWSER ?? 'webkit';
const profile = process.env.NOON_PLAYGROUND_PROFILE ?? 'mobile-dpr2';
const profileGateMode = process.env.NOON_SHOWCASE_PROFILE_GATE ?? 'disabled';
const profilePreparation = process.env.NOON_GALLERY_PREPARATION_TIMING === '1';
assert.ok(['disabled', 'baseline', 'strict'].includes(profileGateMode), 'NOON_SHOWCASE_PROFILE_GATE must be disabled, baseline, or strict');
const port = Number(process.env.NOON_GALLERY_PORT ?? 4197);
const external = process.env.NOON_GALLERY_BASE;
const base = external ?? `http://127.0.0.1:${port}/web/`;
const artifacts = path.resolve(root, process.env.NOON_PLAYGROUND_MATRIX_ARTIFACTS ??
  `browser-smoke-artifacts/gallery/${browserName}-${profile}`);
const stringify = value => JSON.stringify(value, (_, v) => typeof v === 'bigint' ? String(v) : v, 2);
const liveMathAnimations = {
  'showcase-latex-create': [['Create', 0, 1], ['Write', 1, 2]],
  'showcase-text-math': [['Write', 0, 1.2]],
};
async function qualifyTriangleHistory(page, duration, initialDiagnostics) {
  await page.evaluate(async () => {
    window.__triangleControlReceipts = [];
    const { ExecutionWorkerClient } = await import('./execution-worker-client.js');
    for (const name of ['pause', 'seek']) {
      const original = ExecutionWorkerClient.prototype[name];
      ExecutionWorkerClient.prototype[name] = async function (...args) {
        const result = await original.apply(this, args);
        window.__triangleControlReceipts.push({ operation: name, requested: args[0] ?? null,
          time: result?.time, playing: result?.playing, receipt: result?.presentationReceipt ?? null });
        return result;
      };
    }
  });
  const captures = [];
  for (const target of [3, 12, 21]) {
    const time = await seekPausedGallery(page, duration, target);
    const metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
    assert.equal(Number(metrics.metrics.time), time,
      'renderer did not settle at the selected authored time');
    const pixels = await page.locator('#scene').screenshot();
    captures.push({ time, pixelSha256: createHash('sha256').update(pixels).digest('hex'),
      metrics: { time: metrics.metrics.time, session: metrics.metrics.presentedSession,
        presentedFrames: metrics.metrics.presentedFrames, objectCount: metrics.metrics.objectCount,
        bufferedDeltas: metrics.metrics.bufferedDeltas },
      preparedRetention: metrics.engineMetrics?.preparedRetention ?? null });
  }
  assert.equal(new Set(captures.map(capture => capture.pixelSha256)).size, 3,
    'Triangle authored checkpoints must render distinct pictures');
  const rewindTime = await seekPausedGallery(page, duration, 12);
  const rewindMetrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
  const rewindPixels = await page.locator('#scene').screenshot();
  const rewindSha256 = createHash('sha256').update(rewindPixels).digest('hex');
  assert.equal(rewindSha256, captures[1].pixelSha256,
    'rewinding to 12 seconds must reproduce the retained canvas picture exactly');
  const receipts = await page.evaluate(() => window.__triangleControlReceipts);
  const pause = receipts.find(receipt => receipt.operation === 'pause');
  assert.equal(pause?.playing, false, 'playback controls did not pause the looping source');
  assert.ok(Number.isSafeInteger(pause?.receipt?.sequence), 'pause lacked its renderer receipt');
  const seeks = receipts.filter(receipt => receipt.operation === 'seek');
  assert.equal(seeks.length, 4);
  for (const [index, receipt] of seeks.entries()) {
    const expected = [3, 12, 21, 12][index];
    assert.equal(receipt.time, expected);
    assert.equal(receipt.receipt?.time, expected);
    assert.equal(receipt.receipt?.session, pause.receipt.session);
    assert.ok(Number.isSafeInteger(receipt.receipt?.sequence));
    if (index > 0) assert.ok(receipt.receipt.sequence > seeks[index - 1].receipt.sequence,
      'renderer receipts must preserve seek order');
  }
  const diagnostics = await page.evaluate(() => window.__noonExampleGallery.executionDiagnostics);
  assert.equal(diagnostics.runRequestId, initialDiagnostics.runRequestId);
  assert.deepEqual(diagnostics.context, initialDiagnostics.context);
  for (const stage of ['source-proof', 'attached', 'source-complete']) {
    assert.equal(diagnostics.events.filter(event => event.stage === stage).length, 1,
      `history control changed ${stage} count`);
  }
  const retention = rewindMetrics.engineMetrics?.preparedRetention;
  assert.ok(retention && retention.reader && retention.pending, 'prepared retention metrics are unavailable');
  assert.ok(retention.reader.historyPayloadBytes <= retention.reader.historyPayloadBudgetBytes,
    'prepared history exceeded its reported payload budget');
  assert.ok(retention.reader.resourceBytes <= retention.reader.resourceBudgetBytes,
    'prepared resources exceeded their reported resource budget');
  assert.equal(retention.pending.batches, 0, 'terminal history seek retained a pending producer batch');
  return { receipts, captures, rewind: { time: rewindTime, pixelSha256: rewindSha256,
    exactCanvasMatch: true, metrics: { time: rewindMetrics.metrics.time,
      session: rewindMetrics.metrics.presentedSession,
      presentedFrames: rewindMetrics.metrics.presentedFrames,
      objectCount: rewindMetrics.metrics.objectCount } } };
}
await mkdir(artifacts, { recursive: true });
let server, httpServerState, browser, runtimeCache;
let completed = false;
const startedAt = performance.now();
const results = [];
const nativeFailureDiagnostics = browserName === 'webkit' && process.platform === 'darwin' &&
  process.env.GITHUB_ACTIONS === 'true';
async function captureWebKitFailure(name, processes) {
  if (!nativeFailureDiagnostics) return null;
  // macOS launches WebContent/GPU XPC services under launchd, outside Node's
  // process tree. Restrict evidence to this Playwright installation on CI.
  const exec = promisify(execFile);
  try {
    return await Promise.all(processes.slice(0, 4).map(async entry => {
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
async function showcaseSourceBytes(relative) {
  if (!external) return readFileSync(path.join(root, 'web', relative));
  const response = await fetch(new URL(relative, base), {
    signal: AbortSignal.timeout(20000), headers: { 'Cache-Control': 'no-cache' },
  });
  assert.ok(response.ok, `${relative}: HTTP ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}
try {
  if (!external) {
    httpServerState = { kind: 'owned-repository-server', port, ready: false, error: null };
    try {
      server = await serveRepository(root, port, { crossOriginIsolated: process.env.NOON_GALLERY_COI === '1' });
      httpServerState.ready = true;
    } catch (error) {
      httpServerState.error = { code: error.code ?? null, message: String(error).slice(0, 300) };
      throw error;
    }
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
  const servedRuntimeBuildIdentity = await json(`runtime-build-identity.json?t=${Date.now()}`);
  await writeFile(path.join(artifacts, 'runtime-build-identity.json'), stringify(servedRuntimeBuildIdentity));
  const runtimeBuildIdentity = validateGalleryRuntimeBuildIdentity(servedRuntimeBuildIdentity, {
    requireSourceRevision: profileGateMode !== 'disabled',
  });
  const revision = external ? await json(`build-info.json?t=${Date.now()}`) : null;
  if (process.env.NOON_GALLERY_REVISION) assert.equal(revision?.commit, process.env.NOON_GALLERY_REVISION);
  const manifests = await Promise.all(galleryManifestPaths.map(relative => json(relative)));
  const showcaseManifest = manifests[3];
  const entries = galleryEntriesFromManifests(manifests);
  if (profileGateMode !== 'disabled') {
    const sources = new Map();
    for (const entry of showcaseManifest.entries) sources.set(entry.path, await showcaseSourceBytes(entry.path));
    validateShowcaseInventory(showcaseManifest.entries, relative => sources.get(relative));
  }
  const selectedIds = process.env.NOON_GALLERY_CASES?.split(',').map(id => id.trim()).filter(Boolean);
  const selectedPool = galleryPoolForProfileGate(entries, showcaseManifest.entries,
    profileGateMode, selectedIds ?? null);
  const shard = galleryShardFromEnvironment(process.env);
  const selectedEntries = shard ? shardGalleryEntries(selectedPool, shard.index, shard.count) : selectedPool;
  assert.ok(selectedEntries.length > 0, "gallery selection is empty");
  const queue = galleryCaseQueue(selectedEntries, browserName);
  await writeFile(path.join(artifacts, 'shard-inventory.json'), stringify({
    schemaVersion: 1,
    browser: browserName,
    profile,
    shard,
    profileGateMode,
    runtimeBuildId: runtimeBuildIdentity.buildId,
    runtimeSourceRevision: runtimeBuildIdentity.sourceRevision,
    sourceCaseCount: entries.length,
    profileGateScope: profileGateMode === 'disabled' ? 'disabled'
      : selectedIds ? 'focused' : 'full-catalog',
    showcaseCatalog: profileGateMode === 'disabled' ? [] : SHOWCASE_PROFILE_EXPECTATIONS.map(({ id, path: sourcePath, sha256 }) =>
      ({ id, path: sourcePath, sha256 })),
    selectedCaseIds: selectedEntries.map(entry => entry.id),
    profileGateShowcaseCaseIds: profileGateMode !== 'disabled'
      ? selectedEntries.filter(entry => SHOWCASE_PROFILE_EXPECTATIONS.some(row => row.id === entry.id)).map(entry => entry.id)
      : [],
    expectedCases: queue.map(({ entry, noJspi }) => ({ id: entry.id, noJspi })),
  }));
  console.log("Gallery case inventory:", selectedEntries.length, "cases", queue.length, "executions");
  const engine = playwright[browserName];
  assert.ok(engine, "unknown gallery browser " + browserName);
  if (browserName !== 'webkit') browser = await engine.launch(playgroundLaunchOptions(browserName));
  const options = profile === 'android' ? playwright.devices['Pixel 7'] : profile.startsWith('mobile') ?
    playwright.devices['iPhone 13'] : { viewport: { width: 1280, height: 900 }, deviceScaleFactor: profile.endsWith('dpr2') ? 2 : 1 };
  let next = 0;
  async function runCase(caseBrowser, { entry, noJspi }) {
    const caseStartedAt = performance.now();
    const caseStartedWallMs = Date.now();
    const context = await caseBrowser.newContext({ ...options });
    await runtimeCache.install(context);
    if (noJspi) await disableAuthoringJspi(context);
    // Observe the existing result envelope without guessing its payload shape.
    // The production parser below validates the semantic descriptor and duration;
    // semantic_execution is an object, not the boolean true.
    // Renderer time may precede completion of an authored static wait.
    await context.addInitScript(({ channel, protocolVersion, profilePublications, profilePreparation }) => {
      if (profilePreparation) globalThis.__NOON_PLAYGROUND_TEST_HOOKS__ = {
        ...(globalThis.__NOON_PLAYGROUND_TEST_HOOKS__ ?? {}),
        authoringRunOptions: () => ({ diagnosticPreparationTiming: true }),
      };
      window.__galleryAuthoringResults = [];
      window.__galleryWorkerMessages = [];
      const profilingRequestId = Number.MAX_SAFE_INTEGER;
      window.Worker = new Proxy(window.Worker, {
        construct(target, args, newTarget) {
          const workerArgs = [...args];
          const workerScriptUrl = new URL(workerArgs[0], window.location.href).href;
          const worker = Reflect.construct(target, workerArgs, newTarget);
          worker.addEventListener('message', event => {
            const { data } = event;
            if (data?.channel === 'noon.render' && data.requestId === profilingRequestId) {
              // This ID is outside the product client's request range. Consume
              // only our initial diagnostic reply before its normal listener.
              event.stopImmediatePropagation();
              if (data.type !== 'metrics' || data.metrics?.publicationStageProfiling !== true) {
                throw new Error('gallery publication profiling did not start');
              }
              return;
            }
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
          if (profilePublications && new URL(workerScriptUrl).pathname.endsWith('/execution-render-worker.js')) {
            // Enable the existing bounded renderer trace before the caller can
            // initialize playback. A later gallery query can miss publication 1.
            worker.postMessage({ channel: 'noon.render', protocolVersion: 1,
              type: 'metrics', requestId: profilingRequestId, profilePublicationStages: true });
          }
          return worker;
        },
      });
    }, { channel: AUTHORING_CHANNEL, protocolVersion: AUTHORING_PROTOCOL_VERSION,
      profilePublications: Object.hasOwn(liveMathAnimations, entry.id), profilePreparation });
    const page = await context.newPage();
    page.setDefaultTimeout(10000);
    const trace = createBrowserFailureTrace();
    const detachTrace = trace.attach(caseBrowser, context, page);
    const expectedProfile = profileGateMode === 'disabled' ? null
      : SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.id === entry.id) ?? null;
    const result = { id: entry.id, noJspi, browserName, profile, revision, browserVersion: caseBrowser.version(),
      runtimeBuildIdentity: { beforeBuildId: runtimeBuildIdentity.buildId, afterBuildId: null, unchanged: false },
      ...(expectedProfile ? { source: { path: expectedProfile.path, sha256: expectedProfile.sha256 } } : {}),
      errors: [], failedRequests: [], samples: [], presentations: [] };
    let audioGestureIssued = false;
    let audioAttachmentObserved = false;
    const observedPublications = new Set();
    const name = `${entry.id}${noJspi ? '-no-jspi' : ''}`;
    const native = createNativeWebKitEvidence({
      enabled: nativeFailureDiagnostics,
      executablePath: playwright.webkit.executablePath(),
      artifacts, prefix: `${name}-failure`, startedAtMs: caseStartedWallMs,
      collect: collectWebKitCrashReports,
    });
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
          trace.setPhase('gallery-navigation', { exampleId: entry.id, noJspi });
          await page.goto(new URL(`index.html?example=${encodeURIComponent(entry.id)}`, base).href,
            { waitUntil: 'domcontentloaded', timeout: 30000 });
          // Remember the content process while alive; post-crash sampling cannot
          // find it. This Node-side observation does not query scene/runtime state.
          await native.observe('gallery-navigation');
          await page.waitForFunction(() => window.__noonExampleGallery !== undefined, null, { timeout: 45000 });
          await native.observe('gallery-shell-ready');
          trace.setPhase('gallery-authored-execution', { exampleId: entry.id, noJspi });
          let completed = false;
          // The existing whole-case deadline bounds startup and natural playback.
          // A second, shorter timer can reject a lesson that is still progressing.
          while (true) {
            const { workerMessages, ...state } = await page.evaluate(profilePublicationStages => {
              const gallery = window.__noonExampleGallery;
              if (!window.__galleryMetricsPending) {
                window.__galleryMetricsPending = true;
                Promise.resolve(gallery.executionMetrics({ profilePublicationStages })).then(value => { window.__galleryMetrics = value; }, error => { window.__galleryMetricsError = String(error); })
                  .finally(() => { window.__galleryMetricsPending = false; });
              }
              return { workerMessages: window.__galleryWorkerMessages ?? [],
                selected: gallery.selectedExampleId, inFlight: gallery.runInFlight,
                transportMode: gallery.transportMode,
                crossOriginIsolated: window.crossOriginIsolated,
                patch: { ...document.querySelector('#patch-status')?.dataset },
                playback: { ...document.querySelector('.playback-controls')?.dataset },
                timeline: { type: document.querySelector('.playback-scrubber')?.type,
                  end: Number(document.querySelector('.playback-scrubber')?.max) },
                playButtonDisabled: document.querySelector('.playback-controls .playback-toggle')?.disabled ?? null,
                text: document.querySelector('#patch-status')?.value,
                runtimeStatus: document.querySelector('#status-text')?.textContent,
                rendererBackend: document.querySelector('#status')?.dataset.rendererBackend,
                renderHost: document.querySelector('#status')?.dataset.renderHost,
                metrics: window.__galleryMetrics ?? null, metricsError: window.__galleryMetricsError,
                executionDiagnostics: document.querySelector('#patch-status')?.dataset.state === 'applied'
                  ? gallery.executionDiagnostics ?? null : null };
            }, Object.hasOwn(liveMathAnimations, entry.id));
            lastWorkerMessages = workerMessages;
            result.state = state;
            if (expectedProfile?.dispositionKind === 'ordinary' &&
                Number(state.playback.elapsedSeconds) > 0 && !result.firstPlaybackControls) {
              result.firstPlaybackControls = { ...state.playback, ...state.timeline };
            }
            assert.equal(state.selected, entry.id);
            assert.notEqual(state.patch.state, 'error', `${state.text}: ${state.runtimeStatus}`);
            assert.equal(state.metricsError, undefined);
            if (expectedProfile?.audioPolicy === 'authored-audio') {
              const attachedEvent = state.executionDiagnostics?.events?.find(event => event.stage === 'attached');
              if (!audioAttachmentObserved && attachedEvent) {
                audioAttachmentObserved = true;
                result.audioAttachment = {
                  runRequestId: state.executionDiagnostics.runRequestId,
                  interpreterGeneration: state.executionDiagnostics.interpreterGeneration,
                  context: state.executionDiagnostics.context,
                  attachedEvent,
                  initiallyPaused: state.playback?.playing === 'false',
                  playButtonDisabled: state.playButtonDisabled,
                };
              }
              const gestureReady = showcaseAudioGestureReady({ expected: expectedProfile,
                patchApplied: state.patch.state === 'applied', runInFlight: state.inFlight,
                diagnostics: state.executionDiagnostics, playback: state.playback,
                playButtonDisabled: state.playButtonDisabled });
              const attached = Boolean(attachedEvent);
              if (!audioGestureIssued && gestureReady) {
                result.audioGesture = {
                  action: 'playback-controls Play button click',
                  issued: true,
                  initiallyPaused: state.playback?.playing === 'false',
                  sample: {
                    runRequestId: state.executionDiagnostics.runRequestId,
                    interpreterGeneration: state.executionDiagnostics.interpreterGeneration,
                    context: state.executionDiagnostics.context,
                    attachedEvent,
                  },
                  playingConfirmed: false,
                };
                audioGestureIssued = true;
                await page.locator('.playback-controls .playback-toggle').click();
              } else if (!audioGestureIssued && attached &&
                  galleryCaseLifecycleReady({ patchApplied: state.patch.state === 'applied',
                    runInFlight: state.inFlight, requiresDisposition: true,
                    diagnostics: state.executionDiagnostics, exampleId: entry.id })) {
                assert.fail(`${entry.id}: attached authored audio lacked an enabled paused Play gesture path`);
              }
              if (audioGestureIssued && state.playback?.playing === 'true') {
                result.audioGesture.playingConfirmed = true;
                result.audioGesture.confirmedSample = {
                  runRequestId: state.executionDiagnostics?.runRequestId,
                  interpreterGeneration: state.executionDiagnostics?.interpreterGeneration,
                  context: state.executionDiagnostics?.context,
                };
              }
            }
            const metric = state.metrics?.metrics;
            for (const presentation of metric?.publicationStageSamples ?? []) {
              const key = `${presentation.session}:${presentation.sequence}`;
              if (!observedPublications.has(key)) {
                observedPublications.add(key);
                result.presentations.push(presentation);
              }
            }
            if (metric && result.samples.at(-1)?.time !== metric.time) result.samples.push({
              time: metric.time, objects: metric.objectCount, frames: metric.presentedFrames,
              ...(entry.id === 'showcase-camera-follows-path' ? {
                intervalMs: metric.presentationIntervalMs,
                intervalSamples: metric.presentationIntervalSamples,
              } : {}),
            });
            if (state.patch.state === 'applied' && !state.inFlight) {
              const authored = parseAuthoringResult(await page.evaluate(() => window.__galleryAuthoringResults.at(-1)));
              const requiresDisposition = authored.semanticExecution?.preparedPlayback === true ||
                (profileGateMode !== 'disabled' && expectedProfile !== null);
              if ((expectedProfile?.audioPolicy !== 'authored-audio' ||
                    result.audioGesture?.playingConfirmed === true) && galleryCaseLifecycleReady({
                patchApplied: state.patch.state === 'applied',
                runInFlight: state.inFlight,
                requiresDisposition,
                diagnostics: state.executionDiagnostics,
                exampleId: entry.id,
              })) {
                completed = true; break;
              }
            }
            await page.waitForTimeout(100);
          }
          assert.ok(completed, `${entry.id}: initial autoplay did not finish`);
          for (const [animation, start, end] of liveMathAnimations[entry.id] ?? []) {
            assert.ok(result.presentations.some(sample => sample.time > start && sample.time < end &&
              sample.presentation > 0 && Number.isFinite(sample.presentedAtMs)),
              `${animation}: live math animation skipped all intermediate frames`);
          }
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
          if (entry.expected_object_count != null) {
            // The lifecycle sample can be the opening picture of the next loop.
            // Inspect the closed endpoint separately through normal controls.
            let endpointMetrics = metrics;
            if (result.state.playback?.controllable === 'true' && result.authoredDuration > 0) {
              const time = await seekPausedGallery(page, result.authoredDuration);
              endpointMetrics = await waitForPublishedGalleryFrame(page, time, result.authoredDuration);
              result.terminalInspection = { time, metrics: endpointMetrics };
            }
            assert.equal(endpointMetrics.metrics.objectCount, entry.expected_object_count);
          }
          if (expectedProfile) {
            result.executionDiagnostics = result.state?.executionDiagnostics ?? await page.evaluate(() =>
              window.__noonExampleGallery?.executionDiagnostics ?? null);
            result.profileGate = {
              mode: profileGateMode,
              ...evaluateShowcaseDiagnostics(expectedProfile, result.executionDiagnostics),
            };
            if (profileGateMode === 'strict') {
              validateShowcaseDiagnostics(expectedProfile, result.executionDiagnostics);
              if (expectedProfile.dispositionKind === 'ordinary') {
                const controls = result.firstPlaybackControls;
                const started = result.executionDiagnostics.events.find(event => event.stage === 'playback-started');
                assert.equal(controls?.type, 'range', 'first playback must show the timeline cursor');
                assert.equal(controls.durationKnown, 'true');
                assert.equal(controls.preparing, 'false');
                assert.equal(controls.end, started.durationSeconds, 'first playback must show the full timeline');
              }
              result.profileGate.status = 'matched';
              delete result.profileGate.reason;
            }
          }
          if (entry.id === 'showcase-gpu-glow' && profileGateMode !== 'disabled') {
            const sourceRun = result.executionDiagnostics;
            assert.ok(Number.isSafeInteger(sourceRun?.runRequestId) &&
              Number.isSafeInteger(sourceRun?.context?.runGeneration),
            'Glow traversal lacks its completed source-run identity');
            await waitForBrowserObservation(page, async ({ exampleId, runRequestId, runGeneration }) => {
              const gallery = window.__noonExampleGallery;
              const metrics = await gallery.executionMetrics();
              const renderer = metrics?.metrics;
              const playback = metrics?.engineMetrics?.preparedPlayback?.observation;
              const diagnostics = gallery.executionDiagnostics;
              const latest = renderer?.latestPresentationSample;
              const afterEnrollment = (playback?.loopCount === 1 && renderer?.time >= 1.75 &&
                latest?.time >= 1.75) ||
                playback?.loopCount >= 2;
              const receiptMatchesClock = latest && latest.time === renderer?.time &&
                Number.isFinite(playback?.time) && latest.time <= playback.time +
                  8 * Number.EPSILON * Math.max(1, Math.abs(playback.time));
              if (gallery.selectedExampleId === exampleId && !gallery.runInFlight && afterEnrollment &&
                  diagnostics?.runRequestId === runRequestId &&
                  diagnostics?.context?.runGeneration === runGeneration &&
                  receiptMatchesClock && latest.session === renderer.presentedSession &&
                  Number.isSafeInteger(latest.sequence) && renderer.presentedFrames > 0) {
                window.__galleryGlowSecondTraversal = {
                  runRequestId, runGeneration, loopCount: playback.loopCount,
                  playbackTime: playback.time,
                  renderer: { time: renderer.time, objectCount: renderer.objectCount,
                    presentedFrames: renderer.presentedFrames, presentedSession: renderer.presentedSession,
                    latestPresentationSample: latest },
                };
                return true;
              }
              return false;
            }, { exampleId: entry.id, runRequestId: sourceRun.runRequestId,
              runGeneration: sourceRun.context.runGeneration }, { timeout: 30000 });
            result.glowSecondTraversal = await page.evaluate(() => window.__galleryGlowSecondTraversal);
            assert.ok(result.glowSecondTraversal,
              'Glow second traversal did not retain a matching Rust state and renderer receipt');
          }
          if (profileGateMode !== 'disabled' && entry.id === 'showcase-triangle-light') {
            result.playbackHistory = await qualifyTriangleHistory(page, result.authoredDuration,
              result.executionDiagnostics);
          }
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
        browserTrace: trace.snapshot(),
        httpServer: httpServerState ?? null,
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
      await native.observe('failure-before-sampling');
      result.failureDiagnostics.nativeSamples = await captureWebKitFailure(name,
        native.snapshots().at(-1)?.processes ?? []);
      result.failureDiagnostics.nativeCrashReports = await native.failure();
      result.failureDiagnostics.nativeProcessSnapshots = native.snapshots();
      result.failureDiagnostics.attemptedPage = await captureAttemptedPage(page);
    } finally {
      clearTimeout(caseTimer);
      if (profilePreparation) {
        result.preparationTimingDiagnostics = await Promise.race([
          page.evaluate(() => window.__noonExampleGallery?.executionPreparationTimingDiagnostics ?? null),
          new Promise(resolve => setTimeout(() => resolve(null), 1000)),
        ]).catch(() => null);
      }
      if (expectedProfile && !Object.hasOwn(result, 'executionDiagnostics')) {
        const snapshot = await Promise.race([
          page.evaluate(() => window.__noonExampleGallery?.executionDiagnostics ?? null),
          new Promise(resolve => setTimeout(() => resolve(null), 1000)),
        ]).catch(() => null);
        result.executionDiagnostics = snapshot;
        result.profileGate ??= {
          mode: profileGateMode,
          ...evaluateShowcaseDiagnostics(expectedProfile, snapshot),
        };
      }
      if (result.runtimeBuildIdentity.afterBuildId === null) {
        let after = null;
        try {
          after = validateGalleryRuntimeBuildIdentity(
            await json(`runtime-build-identity.json?t=${Date.now()}`), {
              requireSourceRevision: profileGateMode !== 'disabled',
            });
          result.runtimeBuildIdentity.afterBuildId = after.buildId;
          assertGalleryRuntimeBuildIdentityUnchanged(runtimeBuildIdentity, after);
          result.runtimeBuildIdentity.unchanged = true;
        } catch (error) {
          if (after !== null) result.runtimeBuildIdentity.after = after;
          result.runtimeBuildIdentity.afterError = String(error);
          result.outcome = 'fail';
          result.failure = result.failure
            ? `${result.failure}; runtime identity check: ${String(error)}`
            : `runtime identity check: ${String(error)}`;
        }
      }
      await page.screenshot({ path: path.join(artifacts, `${name}.png`), timeout: 5000 }).catch(() => {});
      result.browserTrace = trace.snapshot();
      detachTrace();
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
  // Headed Firefox cases share one display; each DOM interaction needs its own
  // foreground window. WebKit also keeps cold WASM/GPU contexts sequential.
  const concurrency = browserName === 'chromium' ? 2 : 1;
  const liveMathCases = [];
  await Promise.all(Array.from({ length: concurrency }, async () => {
    while (next < queue.length) {
      const spec = queue[next++];
      // The live reveal probe observes a single gallery's wall-clock playback.
      // Other cold software-GPU/WASM contexts can consume its entire interval.
      if (liveMathAnimations[spec.entry.id]) liveMathCases.push(spec);
      else await check(spec);
    }
  }));
  for (const spec of liveMathCases) await check(spec);
  assert.equal(results.length, queue.length, 'incomplete inventory');
  assert.deepEqual(
    results.map(({ id, noJspi }) => id + ":" + noJspi).sort(),
    queue.map(({ entry, noJspi }) => entry.id + ":" + noJspi).sort(),
    'missing or duplicated case execution in this shard',
  );
  const failed = results.filter(result => result.outcome !== 'pass');
  assert.deepEqual(failed.map(result => [result.id, result.noJspi, result.failure]), [], 'gallery runtime failures');
  console.log(`All ${selectedEntries.length} ${selectedIds ? 'selected' : 'selectable'} examples and ${queue.length - selectedEntries.length} no-JSPI controls passed.`);
  completed = true;
} finally {
  if (!completed && httpServerState) {
    await writeFile(path.join(artifacts, 'http-server-diagnostics.json'), stringify(httpServerState));
  }
  await writeFile(path.join(artifacts, 'results.json'), stringify(results));
  const runtimeResources = { elapsedMs: performance.now() - startedAt, ...runtimeCache?.stats() };
  await writeFile(path.join(artifacts, 'runtime-resources.json'), stringify(runtimeResources));
  console.log('Gallery runtime resources:', stringify(runtimeResources));
  await browser?.close();
  if (typeof server?.close === 'function') await server.close();
}
