import assert from "node:assert/strict";
import test from "node:test";
import { fixedStressSampleTimes, classifyStressPhase, validateStressReport } from "./retained-dynamic-stress-perf-lib.mjs";
const options = { transportMode: "shared", rendererBackend: "WebGPU", sampleHz: 60 };
function complete() {
  return { schemaVersion: 2, setup: { warmupFrames: 0 }, scene: { objects: 626 },
    environment: { rendererBackend: "WebGPU", targetHz: 60 },
    execution: { mode: "semantic", transportMode: "shared", sourceContinuation: true,
      sourceCompleted: true, authoredDuration: 5, firstMeasuredTime: 1 / 60, lastMeasuredTime: 5 },
    cadence: { frames: 300, effective: { effectiveFps: 59.8 } },
    samples: fixedStressSampleTimes().slice(1).map(sceneTime => ({ sceneTime, advanceRoundTripMs: 3 })) };
}
test("complete fixed samples preserve all ten authored phases and measured timings", () => {
  const phases = validateStressReport(complete(), options);
  assert.equal(phases.length, 10);
  assert.equal(phases.reduce((count, phase) => count + phase.samples, 0), 300);
  assert.ok(phases.every(phase => phase.advanceRoundTripMs.p95 === 3));
  assert.equal(classifyStressPhase(0.35), "create-grid");
  assert.equal(classifyStressPhase(5), "final-wave");
  assert.equal(classifyStressPhase(5.01), null);
});
test("optional physical cadence floor rejects an FPS regression", () => {
  validateStressReport(complete(), { ...options, minimumEffectiveFps: 55 });
  const report = complete();
  report.cadence.effective.effectiveFps = 54.9;
  assert.throws(
    () => validateStressReport(report, { ...options, minimumEffectiveFps: 55 }),
    /below required 55\.00/,
  );
});
test("reject truncated, misrouted, incomplete, malformed and cheaper workloads", () => {
  const mutations = [
    report => report.samples.pop(),
    report => report.samples[20].sceneTime = 4,
    report => report.samples[20].advanceRoundTripMs = NaN,
    report => report.samples[20].advanceRoundTripMs = -1,
    report => report.execution.sourceCompleted = false,
    report => report.execution.sourceContinuation = false,
    report => report.execution.authoredDuration = 4,
    report => report.execution.lastMeasuredTime = 4,
    report => report.execution.mode = "legacy",
    report => report.execution.transportMode = "transferable",
    report => report.environment.rendererBackend = "WebGL2",
    report => report.scene.objects = 10,
    report => report.setup.warmupFrames = 30,
  ];
  for (const mutate of mutations) {
    const report = complete(); mutate(report);
    assert.throws(() => validateStressReport(report, options));
  }
});
test("coarse runs must still measure every authored phase", () => {
  const report = complete();
  report.environment.targetHz = 1;
  report.execution.firstMeasuredTime = 1;
  report.cadence.frames = 5;
  report.samples = fixedStressSampleTimes(5, 1).slice(1).map(sceneTime => ({ sceneTime, advanceRoundTripMs: 1 }));
  assert.throws(() => validateStressReport(report, { ...options, sampleHz: 1 }), /must be sampled/);
});
