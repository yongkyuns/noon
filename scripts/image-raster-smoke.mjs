// Real direct Rust/WASM and Python-worker image rendering; no synthetic frame path.
import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import playwright from 'playwright';
import { PNG } from 'pngjs';
import { serveRepository } from './browser-test-server.mjs';
import { browserArgs } from './manim-raster-support.mjs';
import { IMAGE_SAMPLE_TIMES, validateDirectImageCapture } from './image-raster-contract.mjs';
import { qualifyPythonImageInputs } from './image-input-qualification.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const artifacts = path.resolve(process.env.NOON_IMAGE_ARTIFACTS ?? 'browser-smoke-artifacts/images');
await mkdir(artifacts, { recursive: true });
const times = IMAGE_SAMPLE_TIMES;
const source = await readFile(path.join(root, 'web/python/examples/raster_image.py'), 'utf8');
const server = await serveRepository(root, Number(process.env.NOON_IMAGE_PORT ?? '4187'));
const reports = [];
const oracleRoot = process.env.NOON_IMAGE_MANIM_DIRECTORY ?? path.join(artifacts, 'manim');
async function compareOracle(png, name, label) {
  const a = PNG.sync.read(png), b = PNG.sync.read(await readFile(path.join(oracleRoot, name)));
  assert.deepEqual([a.width, a.height], [b.width, b.height]);
  let sum = 0, outliers = 0, maximum = 0;
  for (let i = 0; i < a.data.length; i += 4) {
    let largest = 0;
    for (let c = 0; c < 3; ++c) {
      const error = Math.abs(a.data[i + c] - b.data[i + c]);
      sum += error; largest = Math.max(largest, error);
    }
    maximum = Math.max(maximum, largest);
    if (largest > 8) ++outliers;
  }
  const result = {label, maximum, mean: sum / (a.width*a.height*3), outlierFraction: outliers / (a.width*a.height)};
  reports.push(result);
  // Whole-frame comparison, no selective masking. A thin rotated-quad coverage
  // difference is allowed; broad filter, placement and opacity errors are not.
  assert.ok(result.mean <= 1 && result.outlierFraction <= 0.008, `Manim image raster mismatch: ${JSON.stringify(result)}`);
}

try {
  for (const backend of (process.env.NOON_IMAGE_BACKENDS ?? 'webgpu,webgl').split(',')) {
    const browser = await playwright.chromium.launch({ headless: true,
      ...(process.env.NOON_CHROMIUM ? { executablePath: process.env.NOON_CHROMIUM } : { channel: 'chromium' }),
      args: browserArgs(backend) });
    try {
      const context = await browser.newContext({ viewport: {width: 256, height: 256} });
      // Optional offline mirror only adapts public dependency access in this harness.
      // Product workers and all image semantics are unchanged.
      if (process.env.NOON_PYODIDE_DIRECTORY) {
        await context.route('https://cdn.jsdelivr.net/pyodide/v314.0.5/full/**', async route => {
          const file = path.basename(new URL(route.request().url()).pathname);
          const body = await readFile(path.join(process.env.NOON_PYODIDE_DIRECTORY, file));
          const contentType = file.endsWith('.wasm') ? 'application/wasm'
            : /\.(mjs|js)$/.test(file) ? 'text/javascript' : file.endsWith('.json') ? 'application/json' : 'application/octet-stream';
          await route.fulfill({ body, contentType, headers: { 'access-control-allow-origin': '*', 'cross-origin-resource-policy': 'cross-origin' } });
        });
      }
      const direct = await context.newPage();
      direct.on('pageerror', error => console.error('direct:', error));
      await direct.goto(`${server.baseUrl}/web/manim-raster-host.html`);
      const captures = await direct.evaluate(async times => {
        const wasm = await import('./pkg/noon_web.js');
        await wasm.default();
        const canvas = new OffscreenCanvas(256, 256);
        const renderer = await wasm.createDirectRasterImageSmokeRenderer(canvas);
        const sleep = () => new Promise(resolve => setTimeout(resolve, 10));
        async function settle(time) {
          for (let attempt = 0; attempt < 80; ++attempt) {
            if (!JSON.parse(renderer.directWakeDirectiveJson(time)).presentNow) return;
            renderer.render();
            await sleep();
          }
          throw new Error('direct image publication did not settle');
        }
        try {
          await settle(0);
          const captures = [];
          for (const time of times) {
            renderer.advanceDirectRealtime(time * 1000);
            await settle(time * 1000);
            const blob = await canvas.convertToBlob({type: 'image/png'});
            captures.push({ requestedTime: time, publishedTime: renderer.time(),
              wake: JSON.parse(renderer.directWakeDirectiveJson(time * 1000)),
              count: renderer.objectCount(), backend: renderer.rendererBackend(),
              png: Array.from(new Uint8Array(await blob.arrayBuffer())) });
          }
          return captures;
        } finally { renderer.free(); }
      }, times);
      for (const [i, capture] of captures.entries()) {
        // Preserve evidence before assertions, including the two distinct clocks.
        const { png, ...observation } = capture;
        reports.push({ kind: 'direct-publication', ...observation });
        await writeFile(path.join(artifacts, `${backend}-direct-${times[i]}.png`), Buffer.from(png));
        validateDirectImageCapture(capture, times[i], backend);
      }
      for (const sampler of ['nearest', 'bilinear', 'bicubic']) {
        for (const opacity of [1, 0.5]) {
          const capture = await direct.evaluate(async ({sampler, opacity}) => {
            const wasm = await import('./pkg/noon_web.js');
            const canvas = new OffscreenCanvas(256, 256);
            const renderer = await wasm.createDirectRasterImageSamplingRenderer(canvas, sampler, opacity);
            try {
              for (let n = 0; n < 80; ++n) {
                if (renderer.render()) break;
                await new Promise(resolve => setTimeout(resolve, 10));
                if (n === 79) throw new Error('filter fixture did not present');
              }
              const blob = await canvas.convertToBlob({type: 'image/png'});
              return Array.from(new Uint8Array(await blob.arrayBuffer()));
            } finally { renderer.free(); }
          }, {sampler, opacity});
          const png = Buffer.from(capture);
          const name = `${sampler}-${opacity.toFixed(1)}.png`;
          await writeFile(path.join(artifacts, `${backend}-${name}`), png);
          await compareOracle(png, name, `${backend}/${sampler}/${opacity}`);
        }
      }
      for (const capture of captures) {
        await compareOracle(Buffer.from(capture.png), `lifecycle-${capture.requestedTime.toFixed(1)}.png`, `${backend}/lifecycle/${capture.requestedTime}`);
      }
      await direct.close();
      console.log(`[PASS] ${backend}: ${captures.length} direct Rust/WASM image frames`);
      const page = await context.newPage();
      page.on('pageerror', error => console.error('Python page:', error));
      page.on('console', message => { if(message.type() === 'error') console.error('Python console:', message.text()); });
      await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
      await page.evaluate(() => {
        const canvas = document.querySelector('#scene');
        canvas.width = canvas.height = 256;
        canvas.style.width = canvas.style.height = '256px';
      });
      await page.waitForFunction(() => window.noonHostRaster);
      const loaded = await page.evaluate(source => window.noonHostRaster.load(source, 4), source);
      console.log('Python loaded:', JSON.stringify(loaded));
      for (const [i, time] of times.entries()) {
        const metrics = await page.evaluate(({i, times}) => window.noonHostRaster.renderThrough(i, times), {i, times});
        assert.equal(metrics.time, time);
        const screenshot = await page.locator('#scene').screenshot();
        await writeFile(path.join(artifacts, `${backend}-python-${time}.png`), screenshot);
        const a = PNG.sync.read(Buffer.from(captures[i].png));
        const b = PNG.sync.read(screenshot);
        assert.deepEqual([b.width, b.height], [a.width, a.height]);
        let maximum = 0, sum = 0;
        for (let n = 0; n < a.data.length; ++n) { const delta = Math.abs(a.data[n] - b.data[n]); maximum = Math.max(maximum, delta); sum += delta; }
        const comparison = {backend, time, maximum, mean: sum / a.data.length};
        reports.push(comparison);
        assert.ok(maximum <= 1, `Rust/Python image pixels differ: ${JSON.stringify(comparison)}`);
      }
      await page.evaluate(() => window.noonHostRaster.close());
      await page.close();
      console.log(`[PASS] ${backend}: paired Python-worker image frames`);
      await qualifyPythonImageInputs({context, baseUrl: server.baseUrl, root, oracleRoot, artifacts, backend, reports});
      await context.close();
    } finally { await browser.close(); }
  }
  await writeFile(path.join(artifacts, 'report.json'), JSON.stringify(reports, null, 2) + '\n');
} catch (error) {
  await writeFile(path.join(artifacts, 'failure.json'), JSON.stringify({
    message: error.message, stack: error.stack,
  }, null, 2) + '\n');
  throw error;
} finally { await writeFile(path.join(artifacts, 'report.json'), JSON.stringify(reports, null, 2) + '\n'); await server.close(); }
