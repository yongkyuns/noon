import assert from "node:assert/strict";
import test from "node:test";

import {
  STRESS_PHASES,
  STRESS_REACTIVATION_TIMES,
  STRESS_FINAL_VISIBLE_OBJECT_COUNT,
  STRESS_SOURCE_SHA256,
  assertSteadyStressTelemetry,
  assertRetainedMorphReactivation,
  classifyStressPhase,
  fixedStressSampleTimes,
  summarizeStressPhases,
} from "./retained-dynamic-stress-perf-lib.mjs";

test("fixed stress samples cover the complete authored five-second loop", () => {
  const samples = fixedStressSampleTimes();
  assert.equal(samples.length, 301);
  assert.equal(samples[0], 0);
  assert.equal(samples.at(-1), 5);
  assert.match(STRESS_SOURCE_SHA256, /^[0-9a-f]{64}$/u);
  assert.equal(STRESS_FINAL_VISIBLE_OBJECT_COUNT, 626);
  assert.equal(STRESS_PHASES[0].start, 0);
  assert.equal(STRESS_PHASES.at(-1).end, 5);
  for (let index = 1; index < STRESS_PHASES.length; index += 1) {
    assert.equal(STRESS_PHASES[index - 1].end, STRESS_PHASES[index].start);
  }
  assert.equal(classifyStressPhase(0.9), "morph-a");
  assert.equal(classifyStressPhase(4.3), "lifecycle-churn");
  assert.equal(classifyStressPhase(4.7), "final-wave");
  assert.equal(classifyStressPhase(5), "final-wave");
});

test("morph resources remain installed when a completed loop is reactivated", () => {
  const rows = STRESS_REACTIVATION_TIMES.map((sceneTime) => ({
    sceneTime,
    dirty: true,
    geometryCacheMisses: 0,
    inlineRenderGeometryCount: 0,
    renderGeometryResourceCount:
      (sceneTime >= 0.9 && sceneTime < 1.45) || (sceneTime >= 2.17 && sceneTime < 2.72)
        ? 600
        : 0,
    uploadBytes: 64,
  }));
  const retained = assertRetainedMorphReactivation(rows);
  assert.deepEqual(retained.morphs.map((phase) => phase.samples), [3, 3]);

  const packedOnEntry = structuredClone(rows);
  packedOnEntry.find((sample) => sample.sceneTime === 0.92).uploadBytes = 13_281_024;
  packedOnEntry.find((sample) => sample.sceneTime === 2.19).uploadBytes = 13_281_024;
  assert.doesNotThrow(() => assertRetainedMorphReactivation(packedOnEntry));

  const regressed = structuredClone(rows);
  regressed.find((sample) => sample.sceneTime === 2.4).geometryCacheMisses = 1;
  assert.throws(
    () => assertRetainedMorphReactivation(regressed),
    /morph-b reactivation must retain installed geometry resources/u,
  );

  const redundantlyUploaded = structuredClone(rows);
  redundantlyUploaded.find((sample) => sample.sceneTime === 1.1).uploadBytes = 13_281_024;
  assert.throws(
    () => assertRetainedMorphReactivation(redundantlyUploaded),
    /morph-a reactivation must upload compact dynamic state/u,
  );
});

test("phase summaries and steady telemetry preserve the dynamic workload", () => {
  const rows = fixedStressSampleTimes().map((sceneTime) => ({
    sceneTime,
    dirty: sceneTime <= 5,
    engineMs: 1,
    transportApplyMs: 2,
    rendererRenderMs: 3,
    totalMs: 6,
    deltaBytes: 10,
    uploadBytes: 64,
    geometryCacheMisses: 0,
    inlineRenderGeometryCount: 0,
    renderGeometryResourceCount:
      (sceneTime >= 0.9 && sceneTime < 1.45) || (sceneTime >= 2.17 && sceneTime < 2.72)
        ? 600
        : 0,
  }));
  const summarize = (values) => ({ count: values.length, max: Math.max(...values) });
  const phases = summarizeStressPhases(rows, summarize);
  assert.equal(phases.length, STRESS_PHASES.length);
  assert.equal(phases.reduce((sum, phase) => sum + phase.samples, 0), rows.length);
  assert.ok(phases.find((phase) => phase.id === "morph-b").dirtySamples > 0);

  const steady = assertSteadyStressTelemetry(rows);
  assert.equal(steady.morphs.length, 2);
  assert.ok(steady.morphs.every((phase) => phase.samples >= 20));
  assert.ok(steady.morphs.every((phase) => phase.maximumInlineRenderGeometryCount === 0));
  assert.ok(steady.finalWave.samples >= 20);
  assert.equal(steady.finalWave.maximumGeometryCacheMisses, 0);
  assert.equal(steady.finalWave.minimumUploadBytes, 64);
});
