import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import playwright from 'playwright';
import { playgroundLaunchOptions } from './playground-browser-support.mjs';
import { createPyodideResourceCache } from './pyodide-resource-cache.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const browserName = process.env.NOON_PLAYGROUND_BROWSER ?? 'chromium';
const profile = process.env.NOON_PLAYGROUND_PROFILE ?? 'desktop-dpr1';
const port = Number(process.env.NOON_PYTHON_FAMILY_TRANSFORM_PORT ?? 4198);
const external = process.env.NOON_GALLERY_BASE;
const base = external ?? `http://127.0.0.1:${port}/web/`;
const artifacts = path.resolve(
  root,
  process.env.NOON_PLAYGROUND_MATRIX_ARTIFACTS ??
    `browser-smoke-artifacts/python-family-transform/${browserName}-${profile}`,
);

const source = `from noon import *


class SyntheticFamilyRoundTrip(Scene):
    def construct(self):
        source = VGroup(Square(), Square(), Square())
        contracted = VGroup(Square(), Square())
        returned = source.copy()
        returned.set_fill(opacity=0)
        contracted.set_fill(opacity=0)

        self.add(source)
        self.wait(0.5)
        self.play(source.animate.set_fill(opacity=0), run_time=0.35)
        self.play(Transform(source, contracted), run_time=1.8)
        source.set_fill(opacity=1)
        self.wait(0.75)
        source.set_fill(opacity=0)
        self.wait(0.35)
        self.play(Transform(source, returned), run_time=1.8)
        self.play(source.animate.set_fill(opacity=1), run_time=0.35)
`;

await mkdir(artifacts, { recursive: true });
let server;
let browser;
let context;
let runtimeCache;
const result = {
  browserName,
  profile,
  errors: [],
};

try {
  if (!external) {
    server = spawn(
      'python3',
      ['-m', 'http.server', String(port), '--bind', '127.0.0.1', '--directory', root],
      { stdio: 'ignore' },
    );
    let ready = false;
    for (let attempt = 0; attempt < 100; attempt += 1) {
      ready = await fetch(base).then(response => response.ok).catch(() => false);
      if (ready) break;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert.ok(ready, 'Python family Transform smoke HTTP server did not start');
  }

  const workerResponse = await fetch(new URL('python-worker.js', base), {
    signal: AbortSignal.timeout(20000),
  });
  assert.ok(workerResponse.ok, 'Python authoring worker source is unavailable');
  runtimeCache = createPyodideResourceCache(await workerResponse.text());

  const engine = playwright[browserName];
  assert.ok(engine, `unknown browser ${browserName}`);
  browser = await engine.launch(playgroundLaunchOptions(browserName));
  const options = profile === 'android'
    ? playwright.devices['Pixel 7']
    : profile.startsWith('mobile')
      ? playwright.devices['iPhone 13']
      : {
          viewport: { width: 1280, height: 900 },
          deviceScaleFactor: profile.endsWith('dpr2') ? 2 : 1,
        };
  context = await browser.newContext({ ...options });
  await runtimeCache.install(context);
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  page.on('pageerror', error => result.errors.push(String(error)));
  page.on('console', message => {
    if (message.type() === 'error') result.errors.push(message.text());
  });

  await page.goto(`${base}?example=compatible-indicate-square`, {
    waitUntil: 'domcontentloaded',
    timeout: 30000,
  });
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined, null, {
    timeout: 45000,
  });
  await page.evaluate((pythonSource) => {
    const editor = document.querySelector('#python-scene-source');
    if (!(editor instanceof HTMLTextAreaElement)) {
      throw new Error('Python scene editor is unavailable');
    }
    editor.value = pythonSource;
    editor.dispatchEvent(new Event('input', { bubbles: true }));
    window.__pythonFamilyTransformDone = false;
    window.__pythonFamilyTransformError = null;
    Promise.resolve(window.__noonExampleGallery.run())
      .catch(error => {
        window.__pythonFamilyTransformError = String(error);
      })
      .finally(() => {
        window.__pythonFamilyTransformDone = true;
      });
  }, source);

  await page.waitForFunction(() => window.__pythonFamilyTransformDone === true, null, {
    timeout: 90000,
  });
  const state = await page.evaluate(() => ({
    runError: window.__pythonFamilyTransformError,
    inFlight: window.__noonExampleGallery.runInFlight,
    patchState: document.querySelector('#patch-status')?.dataset.state,
    patchText: document.querySelector('#patch-status')?.value,
  }));
  result.state = state;
  assert.equal(state.runError, null, state.runError ?? undefined);
  assert.equal(state.inFlight, false, 'synthetic Python family Transform run did not settle');
  assert.notEqual(state.patchState, 'error', state.patchText);
  assert.equal(state.patchState, 'applied', state.patchText);

  const metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
  result.metrics = metrics;
  assert.ok(Number(metrics?.metrics?.presentedFrames) > 0, 'synthetic scene rendered no frames');
  assert.deepEqual(result.errors, []);
  result.outcome = 'pass';
} catch (error) {
  result.outcome = 'fail';
  result.failure = String(error);
  throw error;
} finally {
  if (context) {
    const pages = context.pages();
    if (pages.length > 0) {
      await pages[0]
        .screenshot({ path: path.join(artifacts, 'python-family-transform.png'), timeout: 5000 })
        .catch(() => {});
    }
  }
  result.runtimeResources = runtimeCache?.stats();
  await writeFile(
    path.join(artifacts, 'python-family-transform.json'),
    JSON.stringify(result, null, 2),
  );
  await context?.close();
  await browser?.close();
  server?.kill('SIGTERM');
}
