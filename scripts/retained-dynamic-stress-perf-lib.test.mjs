import assert from "node:assert/strict";
import test from "node:test";
import {
  STRESS_PERFORMANCE_CASES,
  fixedStressSampleTimes,
  classifyStressPhase,
  summarizeStressCases,
  validateStressReport,
} from "./retained-dynamic-stress-perf-lib.mjs";
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
test("performance cases separate cold morph activation from steady playback", () => {
  const report = complete();
  const cases = summarizeStressCases(report.samples);
  assert.deepEqual(cases.map(performanceCase => performanceCase.id),
    STRESS_PERFORMANCE_CASES.map(performanceCase => performanceCase.id));
  const byId = Object.fromEntries(cases.map(performanceCase => [performanceCase.id, performanceCase]));

  assert.equal(byId["morph-a-activation"].samples, 2);
  assert.equal(byId["morph-a-steady"].samples, 31);
  assert.equal(byId["morph-b-activation"].samples, 2);
  assert.equal(byId["morph-b-steady"].samples, 31);
  assert.equal(byId.turbulence.samples, 27);
  assert.equal(byId["lifecycle-churn"].samples, 63);

  assert.equal(byId["morph-a-activation"].firstSceneTime, 0.9);
  assert.equal(byId["morph-a-activation"].lastSceneTime, 55 / 60);
  assert.equal(byId["morph-a-steady"].firstSceneTime, 56 / 60);
  assert.equal(byId["morph-b-activation"].firstSceneTime, 131 / 60);
  assert.equal(byId["morph-b-steady"].firstSceneTime, 133 / 60);
  assert.ok(cases.every(performanceCase => performanceCase.advanceRoundTripMs.p95 === 3));
});
test("activation outlier cannot be hidden by a fast steady morph case", () => {
  const report = complete();
  const firstMorphA = report.samples.find(sample => classifyStressPhase(sample.sceneTime) === "morph-a");
  firstMorphA.advanceRoundTripMs = 80;
  const cases = Object.fromEntries(
    summarizeStressCases(report.samples).map(performanceCase => [performanceCase.id, performanceCase]),
  );
  assert.equal(cases["morph-a-activation"].advanceRoundTripMs.p95, 80);
  assert.equal(cases["morph-a-steady"].advanceRoundTripMs.p95, 3);
});
test("steady morph regression cannot be hidden by a fast activation case", () => {
  const report = complete();
  const morphA = report.samples.filter(sample => classifyStressPhase(sample.sceneTime) === "morph-a");
  for (const sample of morphA.slice(2)) sample.advanceRoundTripMs = 40;
  const cases = Object.fromEntries(
    summarizeStressCases(report.samples).map(performanceCase => [performanceCase.id, performanceCase]),
  );
  assert.equal(cases["morph-a-activation"].advanceRoundTripMs.p95, 3);
  assert.equal(cases["morph-a-steady"].advanceRoundTripMs.p95, 40);
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
