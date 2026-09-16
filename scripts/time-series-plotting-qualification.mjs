// Real Python worker versus direct typed Rust/WASM at the same authored times.
// Only this external test harness projects pixels and drives explicit samples.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(root, "plotting-artifacts/time-series");
const source = await readFile(path.join(root, "web/python/examples/time_series_plotting.py"), "utf8");
const checkpoints = [0.15, 0.6, 1.8, 4.5, 6];
// Include exact segment boundaries so the normal direct realtime host can
// re-anchor on source resumption without charging setup to the next interval.
const driveTimes = [0, 0.15, 0.3, 0.6, 0.9, 1.2, 1.8, 2.4, 4.2, 4.5, 6];
const report = { pythonSourceSha256: createHash("sha256").update(source).digest("hex"), backends: [] };
const server = await serveRepository(root, 4198);
await mkdir(output, { recursive: true });

async function capture(context, language, backend) {
  const page = await context.newPage();
  page.setDefaultTimeout(90_000);
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  try {
    await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
    await page.waitForFunction(() => window.noonHostRaster);
    await page.evaluate(async ({ language, source }) => {
      const canvas = document.querySelector("#scene");
      window.timeSeriesErrors = [];
      if (language === "rust-wasm") {
        const wasm = await import("./pkg/noon_web.js");
        await wasm.default();
        const renderer = await wasm.createTimeSeriesPlottingRenderer(canvas.transferControlToOffscreen());
        window.timeSeriesRenderer = renderer;
        renderer.resize(960, 540);
        window.sampleTimeSeries = async time => {
          if (time > 0) renderer.advanceDirectRealtime(time * 1000);
          for (let attempt = 0; attempt < 100; attempt++) {
            const wake = JSON.parse(renderer.directWakeDirectiveJson(time * 1000));
            if (!wake.presentNow) {
              return { time: renderer.time(), objectCount: renderer.objectCount(),
                backend: renderer.rendererBackend(), drawCalls: renderer.lastDrawCalls() };
            }
            if (!renderer.render()) await new Promise(resolve => setTimeout(resolve, 10));
          }
          throw new Error("direct time-series publication did not settle");
        };
        return;
      }
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
      const client = new PythonAuthoringClient();
      let resolveAttached, rejectAttached;
      const attached = new Promise((resolve, reject) => {
        resolveAttached = resolve;
        rejectAttached = reject;
      });
      const execution = new AuthoringExecutionClient(canvas, {
        onError(error) { window.timeSeriesErrors.push(String(error)); rejectAttached(error); },
      });
      window.timeSeriesClient = client;
      window.timeSeriesExecution = execution;
      const authored = client.run(source, {}, {
        async onSemanticContinuation(registration) {
          await execution.startSemanticExecution(registration.semanticExecution, {
            authoringClient: client, transportMode: "transferable", pacing: "external_samples",
            loopDurationSeconds: registration.duration,
          });
          resolveAttached();
        },
      });
      authored.catch(error => { window.timeSeriesErrors.push(String(error)); rejectAttached(error); });
      await attached;
      window.sampleTimeSeries = async time => {
        await execution.sampleToAuthoredTime(time);
        if (time === 6) {
          const completed = await authored;
          if (Math.abs(completed.duration - 6) > 1e-6) throw new Error("wrong data playback duration");
        }
        for (let attempt = 0; attempt < 200; attempt++) {
          if (window.timeSeriesErrors.length) throw new Error(window.timeSeriesErrors.join("; "));
          const { metrics } = await execution.metrics();
          if (metrics.ready && metrics.retained && metrics.presentedFrames > 0 &&
              Math.abs(metrics.time - time) < 1e-6) return metrics;
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        throw new Error(`Python time-series sample ${time} was not presented`);
      };
    }, { language, source });
    const captures = [];
    for (const time of driveTimes) {
      const metrics = await page.evaluate(time => window.sampleTimeSeries(time), time);
      assert.ok(Math.abs(metrics.time - time) < 1e-6, `${language}: ${JSON.stringify(metrics)}`);
      assert.equal(metrics.backend, backend);
      assert.ok(metrics.drawCalls > 0);
      if (!checkpoints.includes(time)) continue;
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const bytes = await page.locator("#scene").screenshot({ path: path.join(output, `${backend}-${language}-${time}.png`) });
      captures.push({ time, metrics, png: PNG.sync.read(bytes) });
    }
    assert.deepEqual(errors, []);
    assert.equal(captures.at(-1).metrics.objectCount, 35);
    return captures;
  } finally {
    await page.close();
  }
}

// Independent data-time oracle, not another implementation used by the demo.
const values = [[0, 0.4], [0.5, 1], [1.5, 1.7], [2, 1.2], [4, 0.6], [7, 2.1], [10, 1.4]];
function assertMarker(png, time) {
  const t = time / 6 * 10;
  const end = Math.max(1, values.findIndex(pair => pair[0] >= t));
  const [a, b] = t === 10 ? values.slice(-2) : [values[end - 1], values[end]];
  const value = a[1] + (b[1] - a[1]) * (t - a[0]) / (b[0] - a[0]);
  const x = t - 5, y = value * 1.6 - 2;
  const cx = Math.round(480 + x * 67.5), cy = Math.round(270 - y * 67.5);
  let yellow = 0;
  for (let dy = -3; dy <= 3; dy++) for (let dx = -3; dx <= 3; dx++) {
    const i = ((cy + dy) * png.width + cx + dx) * 4;
    if (png.data[i] > png.data[i + 2] + 40 && png.data[i + 1] > png.data[i + 2] + 40) yellow++;
  }
  assert.ok(yellow > 5, `marker is not at data time ${t}: ${yellow} yellow pixels`);
}

try {
  for (const selected of ["webgpu", "webgl"]) {
    const backend = selected === "webgpu" ? "WebGPU" : "WebGL2";
    const result = { backend, samples: [] };
    report.backends.push(result);
    const browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs(selected) });
    try {
      const context = await browser.newContext({ viewport: { width: 1000, height: 600 } });
      const rust = await capture(context, "rust-wasm", backend);
      const python = await capture(context, "python", backend);
      for (let index = 0; index < rust.length; index++) {
        const a = rust[index], b = python[index];
        assert.equal(a.png.width, 960);
        assert.equal(a.png.height, 540);
        assert.equal(b.png.width, a.png.width);
        assert.equal(b.png.height, a.png.height);
        assertMarker(a.png, a.time);
        assertMarker(b.png, b.time);
        let differingPixels = 0;
        for (let i = 0; i < a.png.data.length; i += 4) {
          if (!a.png.data.subarray(i, i + 4).equals(b.png.data.subarray(i, i + 4))) differingPixels++;
        }
        result.samples.push({ time: a.time, differingPixels, rust: a.metrics, python: b.metrics });
        assert.equal(differingPixels, 0, `paired ${backend} data-time pixels differ at ${a.time}`);
      }
      result.passed = true;
      console.log(`[PASS] ${backend}: numeric labels and five paired data-time frames`);
    } catch (error) {
      result.passed = false;
      result.error = error.stack ?? String(error);
      console.error(result.error);
    } finally {
      await browser.close();
    }
  }
} finally {
  await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2) + "\n");
  await server.close();
}
assert.ok(report.backends.length === 2 && report.backends.every(r => r.passed), "time-series plotting qualification failed");
