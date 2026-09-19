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
import { assertGapPixels, gapArgumentSource, gapCheckpoints } from "./gapped-plotting-checks.mjs";

import { assertLiveCoordinatePixels } from "./live-coordinate-checks.mjs";
import { assertSampleReceipt, pythonPresentedTime } from "./plotting-sample-contract.mjs";
import { animatedPlottingCases, animatedPresentedTime, assertAnimatedPlotPixels,
  assertAnimatedPlotHolds } from "./animated-plotting-checks.mjs";
const mode = process.argv[2] ?? "single";
const liveCoordinates = mode === "live";
const animated = animatedPlottingCases[mode];
const runTime = animated?.duration ?? (liveCoordinates ? 1.5 : 6);
const cases = {
  live: ["createLiveCoordinatePlottingRenderer", "live_coordinate_plotting", "live-coordinates", 17],
  single: ["createTimeSeriesPlottingRenderer", "time_series_plotting", "time-series", 35],
  synchronized: ["createSynchronizedPlottingRenderer", "synchronized_plotting", "synchronized-series", 43],
  gapped: ["createGappedPlottingRenderer", "gapped_plotting", "gapped-series", 51],
};
for (const [name, example] of Object.entries(animatedPlottingCases)) {
  cases[name] = [example.factory, example.source, example.output, example.count];
}
assert.ok(Object.hasOwn(cases, mode) && process.argv.length <= 3,
  "expected single, synchronized, gapped, live, coordinates, or number-line");
const synchronized = mode === "synchronized" || mode === "gapped";
const gapped = mode === "gapped";
const [factoryName, exampleName, outputName, expectedObjectCount] = cases[mode];
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(root, `plotting-artifacts/${outputName}`);
const source = await readFile(path.join(root, `web/python/examples/${exampleName}.py`), "utf8");
const checkpoints = animated?.checkpoints ?? (liveCoordinates ? [0, 0.125, 0.25, 0.5, 0.75, 1, 1.25, 1.375, 1.5]
  : gapped ? gapCheckpoints : [0.15, 0.6, 1.8, 4.5, 6]);
// Include exact segment boundaries so the normal direct realtime host can
// re-anchor on source resumption without charging setup to the next interval.
const data = [[0, 0.4], [0.5, 1], [1.5, 1.7], [2, 1.2], [4, 0.6], [7, 2.1], [10, 1.4]];
const recordings = [
  [[0, 0.4], [0.5, 1], [2, 0.8], [5, 1.2], [10, 0.6]],
  [[-1, 2.5], [1.5, 1.9], [4, 2.4], [8, 1.8], [12, 2.6]],
];
const unionTimes = [...new Set([0, 10, ...recordings.flat().map(([t]) => t)
  .filter(t => t > 0 && t < 10)])].sort((a, b) => a - b);
const boundaries = synchronized ? unionTimes : data.map(([t]) => t);
const driveTimes = [...new Set([0, ...checkpoints, ...(animated?.boundaries ?? (liveCoordinates ? [0.25, 0.75, 1.25, 1.5] : boundaries.map(t => (t / 10) * 6)))])]
  .sort((a, b) => a - b);
const report = { mode, pythonSourceSha256: createHash("sha256").update(source).digest("hex"), backends: [] };
const server = await serveRepository(root, 4198);
await mkdir(output, { recursive: true });

async function bounded(promise, label) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label} timed out`)), 90_000);
    })]);
  } finally { clearTimeout(timer); }
}

async function capture(context, language, backend) {
  const page = await context.newPage();
  page.setDefaultTimeout(90_000);
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  try {
    await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
    await page.waitForFunction(() => window.noonHostRaster);
    await bounded(page.evaluate(async ({ language, source, factoryName, runTime, liveCoordinates, mode }) => {
      const canvas = document.querySelector("#scene");
      window.timeSeriesErrors = [];
      if (language === "rust-wasm") {
        const wasm = await import("./pkg/noon_web.js");
        await wasm.default();
        const renderer = await wasm[factoryName](canvas.transferControlToOffscreen());
        window.timeSeriesRenderer = renderer;
        renderer.resize(960, 540);
        window.sampleTimeSeries = async time => {
          let wallTimeMs = time * 1000;
          if (time > 0) renderer.advanceDirectRealtime(wallTimeMs);
          for (let attempt = 0; attempt < 100; attempt++) {
            const wake = JSON.parse(renderer.directWakeDirectiveJson(wallTimeMs));
            if (!wake.presentNow) {
              const actual = renderer.time();
              // Seconds -> milliseconds -> reanchored seconds can round just
              // below an interval boundary. Cross only that representational
              // gap; a substantive clock mismatch still fails the assertions.
              if (actual < time && time - actual <= 16 * Number.EPSILON * Math.max(1, time)) {
                wallTimeMs += Number.EPSILON * Math.max(1, wallTimeMs);
                renderer.advanceDirectRealtime(wallTimeMs);
                continue;
              }
              return { time: actual, objectCount: renderer.objectCount(),
                backend: renderer.rendererBackend(), drawCalls: renderer.lastDrawCalls(),
                cadence: wake.cadence, delayMs: wake.delayMs };
            }
            if (!renderer.render()) await new Promise(resolve => setTimeout(resolve, 10));
          }
          throw new Error("direct time-series publication did not settle");
        };
        return;
      }
      const { animatedPlottingCases, animatedPresentedTime } = await import("../scripts/animated-plotting-samples.mjs");
      const { assertSampleReceipt, isPresentedSample, pythonPresentedTime } =
        await import("../scripts/plotting-sample-contract.mjs");
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
        const sample = await execution.sampleToAuthoredTime(time);
        assertSampleReceipt(sample, time);
        const expectedFrameTime = animatedPlottingCases[mode] ? animatedPresentedTime(time, mode) : pythonPresentedTime(time, liveCoordinates);
        if (time === runTime) {
          const completed = await authored;
          if (Math.abs(completed.duration - runTime) > 1e-6) throw new Error("wrong data playback duration");
        }
        let lastMetrics;
        for (let attempt = 0; attempt < 200; attempt++) {
          if (window.timeSeriesErrors.length) throw new Error(window.timeSeriesErrors.join("; "));
          const { metrics } = await execution.metrics();
          lastMetrics = { time: metrics.time, ready: metrics.ready,
            retained: metrics.retained, presentedFrames: metrics.presentedFrames };
          if (isPresentedSample(metrics, expectedFrameTime)) {
            // Keep this external report to scalar raster observations. Full
            // diagnostics include BigInt publication identities, not scene data
            // that this pixel comparison needs to serialize.
            return { time: metrics.time, objectCount: metrics.objectCount,
              backend: metrics.backend, drawCalls: metrics.drawCalls,
              presentedFrames: metrics.presentedFrames, sampleTime: sample.time,
              sourceCompleted: sample.sourceCompleted };
          }
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        throw new Error(`Python sample ${time} expected presented time ${expectedFrameTime}; ` +
          `acknowledged ${sample.time}, renderer ${JSON.stringify(lastMetrics)}`);
      };
    }, { language, source, factoryName, runTime, liveCoordinates, mode }), `${language} attachment`);
    const captures = [];
    for (const time of driveTimes) {
      const metrics = await bounded(page.evaluate(time => window.sampleTimeSeries(time), time),
        `${language} sample ${time}`);
      const animatedWait = animated?.waits.find(([start, end]) => time > start && time < end);
      const quietWait = animatedWait || (liveCoordinates && (time < 0.25 || (time > 1.25 && time < 1.5)));
      if (language === "python") {
        assertSampleReceipt({ time: metrics.sampleTime }, time);
        assert.ok(Math.abs(metrics.time - (animated ? animatedPresentedTime(time, mode) : pythonPresentedTime(time, liveCoordinates))) < 1e-6,
          `Python renderer time: ${JSON.stringify(metrics)}`);
        if (time === runTime) assert.equal(metrics.sourceCompleted, true);
      } else if (quietWait) {
        // A quiet wait sleeps until its deadline; an early host tick must not
        // force a fresh semantic/render frame. Verify the exact timer instead
        // of relabeling the old frame as a newly evaluated requested time.
        const [start, end] = animatedWait ?? (time < 0.25 ? [0, 0.25] : [1.25, 1.5]);
        assert.equal(metrics.time, start);
        assert.equal(metrics.cadence, "timer");
        assert.ok(Math.abs(metrics.delayMs - (end - time) * 1000) < 1e-6,
          `wrong quiet-wait deadline: ${JSON.stringify(metrics)}`);
      } else {
        assert.ok(Math.abs(metrics.time - time) < 1e-6, `${language}: ${JSON.stringify(metrics)}`);
      }
      assert.equal(metrics.backend, backend);
      if (!animated || time > 0) assert.ok(metrics.drawCalls > 0);
      if (!checkpoints.includes(time)) continue;
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const bytes = await page.locator("#scene").screenshot({ path: path.join(output, `${backend}-${language}-${time}.png`) });
      captures.push({ time, metrics, png: PNG.sync.read(bytes) });
    }
    if (animated) assertAnimatedPlotHolds(captures, mode);
    if (liveCoordinates) {
      assert.equal(captures[0].time, 0);
      assert.equal(captures[1].time, 0.125);
      assert.deepEqual(captures[0].png.data, captures[1].png.data,
        "quiet wait changed the image before coordinate construction");
      const finalAnimation = captures.find(capture => capture.time === 1.25);
      for (const capture of captures.filter(capture => capture.time > 1.25)) {
        assert.deepEqual(capture.png.data, finalAnimation.png.data,
          `final quiet wait changed the image at ${capture.time}`);
      }
    }
    assert.deepEqual(errors, []);
    assert.equal(captures.at(-1).metrics.objectCount, expectedObjectCount);
    return captures;
  } finally {
    await page.close();
  }
}

async function validateArguments(context) {
  const page = await context.newPage();
  try {
    await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
    return await bounded(page.evaluate(async extraChecks => {
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      const client = new PythonAuthoringClient();
      try {
        const result = await client.run(`from noon import *
class PresentationValidation(Scene):
    def construct(self):
        axes = Axes((0, 10, 2), (0, 2.5, 0.5), x_length=10, y_length=4)
        labels = axes.x_axis.label_plan((-0.004, -0.006), decimal_places=2, exclude_zero=False)
        assert tuple(x.text for x in labels) == ("0.00", "-0.01")
        assert axes.x_axis.label_plan(()) == ()
        for precision in (-1, 13, 2**32, 2**32 + 1):
            try:
                axes.x_axis.label_plan(decimal_places=precision)
            except ValueError:
                pass
            else:
                raise AssertionError("invalid precision was accepted")
        before = axes.c2p(0, 0)
        for samples in ((), ((1, 2),), ((1, 2), (1, 3)), ((2, 1), (1, 2)), ((0, 0), (1, float("nan")))):
            try:
                axes.time_series_plan(samples, run_time=6)
            except ValueError:
                pass
            else:
                raise AssertionError("invalid timestamps were accepted")
        assert axes.c2p(0, 0) == before
        plan = axes.time_series_plan(((0, 1), (1, 2), (10, 1)), run_time=5)
        assert plan.durations == (0.5, 4.5)
        assert plan.points[0] == axes.c2p(0, 1)
        axes.shift((1, 0))
        assert plan.points[0] != axes.c2p(0, 1)
        self.add(axes)
`);
        if (result.duration !== 0) throw new Error("preparation checks invented playback");
        if (extraChecks) {
          const extra = await client.run(extraChecks);
          if (extra.duration !== 0) throw new Error("synchronized preparation invented playback");
        }
        return { passed: true, synchronized: Boolean(extraChecks) };
      } finally { client.terminate(); }
    }, gapped ? gapArgumentSource : synchronized ? synchronizedArgumentSource : ""), "Pyodide argument validation");
  } finally { await page.close(); }
}

// Independent data-time/pixel oracle, never called by either scene.
function regionCount(png, x, y, predicate) {
  const cx = Math.round(480 + x * 67.5), cy = Math.round(270 - y * 67.5);
  let count = 0;
  for (let dy = -3; dy <= 3; dy++) for (let dx = -3; dx <= 3; dx++) {
    const i = ((cy + dy) * png.width + cx + dx) * 4;
    if (predicate(png.data[i], png.data[i + 1], png.data[i + 2])) count++;
  }
  return count;
}
function assertMarker(png, time) {
  const t = time / 6 * 10;
  const end = Math.max(1, data.findIndex(pair => pair[0] >= t));
  const [a, b] = t === 10 ? data.slice(-2) : [data[end - 1], data[end]];
  const point = timestamp => [timestamp - 5,
    (a[1] + (b[1] - a[1]) * (timestamp - a[0]) / (b[0] - a[0])) * 1.6 - 2];
  const [x, y] = point(t);
  const yellow = regionCount(png, x, y, (r, g, b) => r > b + 40 && g > b + 40);
  assert.ok(yellow > 5, `marker is not at data time ${t}: ${yellow} yellow pixels`);
  const green = regionCount(png, x, -1.8, (r, g, b) => g > r + 25 && g > b + 25);
  assert.ok(green > 2, `cursor is not at data time ${t}`);
  const blue = (r, g, b) => b > r + 35 && g > r + 20;
  assert.ok(regionCount(png, ...point((a[0] + t) / 2), blue) > 2,
    `revealed curve has not reached data time ${t}`);
  if (t < b[0]) assert.equal(regionCount(png, ...point((t + b[0]) / 2), blue), 0,
    `curve reveals future data beyond time ${t}`);
}


const synchronizedArgumentSource = `from noon import *
from _manim_plotting import _owned, _array
from _noon_errors import engine_call
class SynchronizedValidation(Scene):
    def construct(self):
        axes = Axes((0, 10, 2), (0, 10, 2), x_length=10, y_length=4)
        rows = (((0, 0), (2, 2), (10, 10)), ((-1, 11), (5, 5), (12, -2)))
        plan = axes.synchronized_series_plan(rows, time_range=(0, 10), run_time=5)
        assert plan.data_times == (0, 2, 5, 10)
        assert plan.durations == (1, 1.5, 2.5)
        assert plan.series_points[0][2] == axes.c2p(5, 5)
        assert plan.series_points[1][1] == axes.c2p(2, 8)
        before = axes.c2p(0, 0)
        for invalid in ((), ((),), (((0, 0), (0, 1)),), (((1, 0), (10, 1)),),
                        (((0, 0), (9, 1)),), (((0, 0), (10, float("nan"))),)):
            try:
                axes.synchronized_series_plan(invalid, time_range=(0, 10), run_time=5)
            except ValueError:
                pass
            else:
                raise AssertionError("invalid recording or uncovered window was accepted")
        with _owned(axes._coordinate_frame()) as frame:
            values = _array((0, 0, 10, 10, 0, 10, 10, 0))
            for counts in ((-2, 2), (2.5, 2), (2**32 + 2, 2), (2, 3), ()):
                try:
                    engine_call(frame.synchronizedSeriesPlan, values, _array(counts), _array((0, 10)), 5)
                except ValueError:
                    pass
                else:
                    raise AssertionError("malformed boundary counts were accepted")
            with _owned(engine_call(frame.synchronizedSeriesPlan, values, _array((2, 2)), _array((0, 10)), 5)) as raw:
                for index in (-1, 0.5, 2, 2**32, float("nan")):
                    try:
                        engine_call(raw.seriesPoints, index)
                    except ValueError:
                        pass
                    else:
                        raise AssertionError("invalid series index was accepted")
        assert axes.c2p(0, 0) == before
        axes.shift((1, 0))
        assert plan.series_points[0][2] != axes.c2p(5, 5)
        self.add(axes)
`;

// Separate oracle for the new case; the single-series oracle remains unchanged.
function assertSynchronizedMarkers(png, time) {
  const t = time / 6 * 10;
  const end = t === 10 ? unionTimes.length - 1 : unionTimes.findIndex(value => value > t);
  const left = unionTimes[end - 1], right = unionTimes[end];
  const predicates = [
    (r, g, b) => b > r + 35 && g > r + 20,
    (r, g, b) => r > g + 20 && g > b + 20,
  ];
  for (let row = 0; row < recordings.length; row++) {
    const point = timestamp => {
      const source = recordings[row];
      const next = Math.max(1, source.findIndex(([x]) => x >= timestamp));
      const [a, b] = [source[next - 1], source[next]];
      const value = a[1] + (b[1] - a[1]) * (timestamp - a[0]) / (b[0] - a[0]);
      return [timestamp - 5, value * 4 / 3 - 2];
    };
    const color = predicates[row];
    assert.ok(regionCount(png, ...point(t), color) > 35,
      `series ${row} marker missing at data time ${t}`);
    assert.ok(regionCount(png, ...point((left + t) / 2), color) > 2,
      `series ${row} has not revealed data through ${t}`);
    if (t < right) assert.equal(regionCount(png, ...point(t + (right - t) * 0.8), color), 0,
      `series ${row} prematurely reveals future data after ${t}`);
  }
  assert.ok(regionCount(png, t - 5, -1.8, (r, g, b) => g > r + 25 && g > b + 25) > 2,
    `shared cursor missing at data time ${t}`);
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
        const oracle = animated ? (png, time) => assertAnimatedPlotPixels(png, time, mode, regionCount) : liveCoordinates ? (png, time) => assertLiveCoordinatePixels(png, time, regionCount) : gapped
          ? (png, time) => assertGapPixels(png, time, recordings, unionTimes, regionCount)
          : synchronized ? assertSynchronizedMarkers : assertMarker;
        oracle(a.png, a.time);
        oracle(b.png, b.time);
        let differingPixels = 0;
        for (let i = 0; i < a.png.data.length; i += 4) {
          if (!a.png.data.subarray(i, i + 4).equals(b.png.data.subarray(i, i + 4))) differingPixels++;
        }
        result.samples.push({ time: a.time, differingPixels, rust: a.metrics, python: b.metrics });
        assert.equal(differingPixels, 0, `paired ${backend} data-time pixels differ at ${a.time}`);
      }
      if (!animated) result.arguments = await validateArguments(context);
      result.passed = true;
      console.log(`[PASS] ${backend} (${mode}): numeric labels and ${checkpoints.length} paired data-time frames`);
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
