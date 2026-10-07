import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import test from "node:test";
import { FrameMetrics } from "./frame-metrics.js";

async function runProfiler({ duration, frames, targetHz, warmupFrames = 0, continuation = false,
  includeSamples = false, frameTimestamps = [] }) {
  const source = (await readFile(new URL("scene-perf.js", import.meta.url), "utf8"))
    .replace(/^import .*;\s*$/gm, "");
  const startOptions = [];
  const advances = [];
  const canvas = { clientWidth: 640, clientHeight: 360, width: 640, height: 360 };
  const status = { value: "", dataset: {} };
  const output = { textContent: "" };
  let now = 0;
  let animationFrame = 0;
  class MockAuthoringClient {
    async ready() { return { mock: true }; }
    async run(_source, _context, { onSemanticContinuation }) {
      if (continuation) await onSemanticContinuation({ semanticExecution: { contextId: "continued" }, duration });
      return { semanticExecution: { contextId: "completed" }, duration };
    }
    terminate() {}
  }
  class MockExecutionClient {
    constructor() { this.mode = "semantic"; this.rendererBackend = "WebGL2"; }
    async startSemanticExecution(_descriptor, options) {
      startOptions.push(options);
      this.loopDurationSeconds = options.loopDurationSeconds ?? 4;
      if (continuation && this.loopDurationSeconds < duration) {
        throw new Error("playback duration is shorter than live handoff duration");
      }
      return { transportMode: "transferable" };
    }
    async state() { return { time: 0 }; }
    async advanceTo(time) {
      advances.push(time);
      if (time > this.loopDurationSeconds) throw new Error("sample exceeds configured loop duration");
      if (duration > 0) {
        return { time, sourceCompleted: time >= (this.loopDurationSeconds ?? 4) };
      }
      return { time, sourceCompleted: false };
    }
    async sampleToAuthoredTime(time) {
      advances.push(time);
      return { time, sourceCompleted: false };
    }
    async metrics() {
      return { metrics: {
        objectCount: 3, presentedFrames: 0, lastDeltaApplyMs: 0, lastRendererCallMs: 0,
        drawCalls: 0, instancesDrawn: 0, bytesUploaded: 0, geometryCacheMisses: 0,
      } };
    }
    terminate() {}
  }
  const reportPromise = runInNewContext(`(async () => {\n${source}\n})()`, {
    URLSearchParams,
    location: { search: `?warmup=${warmupFrames}&frames=${frames}&targetHz=${targetHz}&includeSamples=${Number(includeSamples)}` },
    document: { querySelector: selector => selector === "#scene" ? canvas : selector === "#status" ? status : output },
    window: { devicePixelRatio: 1 },
    navigator: { userAgent: "mock-browser" },
    performance: { now: () => ++now },
    requestAnimationFrame: callback => callback(frameTimestamps[animationFrame++] ?? now),
    fetch: async () => ({ ok: true, text: async () => "# mocked authoring source" }),
    ProvenancedPythonAuthoringClient: MockAuthoringClient,
    AuthoringExecutionClient: MockExecutionClient,
    BrowserJankMonitor: class { start() {} stop() {} summary() { return []; } },
    FrameMetrics,
    shouldSampleRendererStageFrame: () => false,
    console: { log() {}, error(error) { throw error; } },
  });
  await reportPromise;
  return { report: JSON.parse(output.textContent), startOptions, advances, status };
}

test("authoring tools cannot restore the deleted frontend identity authority", async () => {
  for (const file of ["authoring-perf.js", "main.js", "scene-perf.js"]) {
    const source = await readFile(new URL(file, import.meta.url), "utf8");
    assert.doesNotMatch(source, /SceneIdentityMap|NoonCanvasPlayer|\.reconcileScene\(/, file);
  }
  for (const file of ["scene-identity.js", "scene-pipeline-perf.mjs"]) {
    await assert.rejects(access(new URL(file, import.meta.url)), { code: "ENOENT" });
  }
});

test("completed predeclared profiling uses source duration; static scenes retain positive playback", async () => {
  const longScene = await runProfiler({ duration: 10, frames: 24, targetHz: 2 });
  assert.equal(longScene.startOptions[0].loopDurationSeconds, 10);
  assert.equal(longScene.report.execution.authoredDuration, 10);
  assert.equal(longScene.report.execution.lastMeasuredTime, 10);
  assert.equal(longScene.advances.at(-1), 10);
  assert.ok(longScene.advances.every(time => time <= 10));

  const staticScene = await runProfiler({ duration: 0, warmupFrames: 2, frames: 5, targetHz: 2 });
  assert.equal(staticScene.startOptions[0].loopDurationSeconds, 3.5);
  assert.equal(staticScene.report.execution.authoredDuration, 0);
  assert.equal(staticScene.report.cadence.frames, 5);
  assert.equal("samples" in staticScene.report, false);
  assert.equal(staticScene.report.execution.lastMeasuredTime, 3.5);
  assert.deepEqual(staticScene.advances, [0, 0.5, 1, 1.5, 2, 2.5, 3, 3.5]);
});

test("opt-in raw timestamps reproduce measured frame gaps without including warmup", async () => {
  const { report } = await runProfiler({ duration: 0, warmupFrames: 1, frames: 3, targetHz: 60,
    includeSamples: true, frameTimestamps: [80, 100, 116.7, 150.1] });
  const timestamps = report.samples.map(sample => sample.frameTimestampMs);
  assert.deepEqual(timestamps, [100, 116.7, 150.1]);
  const gaps = timestamps.slice(1).map((time, index) => time - timestamps[index]);
  assert.ok(Math.abs(gaps[0] - 16.7) < 1e-9);
  assert.ok(Math.abs(gaps[1] - 33.4) < 1e-9);
  assert.equal(report.cadence.frameIntervalMs.max, Math.max(...gaps));
  assert.equal(report.cadence.frameIntervalMs.mean, (gaps[0] + gaps[1]) / 2);
  assert.equal(report.cadence.effective.longFrames, 1);
});

test("source-owned continuation keeps external sampling with its initial handoff and sampling horizon", async () => {
  const continuation = await runProfiler({ duration: 10, frames: 3, targetHz: 2, continuation: true });
  assert.equal(continuation.startOptions[0].pacing, "external_samples");
  assert.equal(continuation.startOptions[0].loopDurationSeconds, 10);
  const longerSampling = await runProfiler({ duration: 1, warmupFrames: 2, frames: 5, targetHz: 2, continuation: true });
  assert.equal(longerSampling.startOptions[0].loopDurationSeconds, 3.5);
});
