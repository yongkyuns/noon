import assert from "node:assert/strict";

export const STRESS_DURATION_SECONDS = 5;
export const STRESS_FINAL_VISIBLE_OBJECT_COUNT = 626;
export const STRESS_SAMPLE_HZ = 60;
export const STRESS_SOURCE_SHA256 = "ae058af41d1ace3a00bd289a4f0a474aa492b2ee22523c17d92b7f0fc49c1527";
// These boundaries are the authored play segments in
// web/python/examples/manim_parity_stress_grid.py. Keep the complete five-second
// loop visible in reports so optimizations cannot benchmark a cheaper excerpt.
export const STRESS_PHASES = Object.freeze([
  { id: "text-intro", start: 0, end: 0.35 },
  { id: "create-grid", start: 0.35, end: 0.9 },
  { id: "morph-a", start: 0.9, end: 1.45 },
  { id: "stagger-a", start: 1.45, end: 1.93 },
  { id: "text-wave-a", start: 1.93, end: 2.17 },
  { id: "morph-b", start: 2.17, end: 2.72 },
  { id: "turbulence", start: 2.72, end: 3.17 },
  { id: "text-wave-b", start: 3.17, end: 3.41 },
  { id: "lifecycle-churn", start: 3.41, end: 4.46 },
  { id: "final-wave", start: 4.46, end: STRESS_DURATION_SECONDS },
]);

// Performance cases deliberately describe user-visible workload slices rather
// than implementation stages. They remain valid if transport, residency, or GPU
// representation changes. The activation cases isolate the first measured frames
// after a morph phase becomes active; steady cases exclude those frames so cold
// preparation cannot hide a steady-state regression (or vice versa).
export const STRESS_PERFORMANCE_CASES = Object.freeze([
  {
    id: "morph-a-activation",
    phase: "morph-a",
    takeFirstSamples: 2,
    intent: "first activation of the first 600-object morph phase",
  },
  {
    id: "morph-a-steady",
    phase: "morph-a",
    skipFirstSamples: 2,
    intent: "steady retained playback after the first morph phase is active",
  },
  {
    id: "morph-b-activation",
    phase: "morph-b",
    takeFirstSamples: 2,
    intent: "first activation of the second 600-object morph phase",
  },
  {
    id: "morph-b-steady",
    phase: "morph-b",
    skipFirstSamples: 2,
    intent: "steady retained playback after the second morph phase is active",
  },
  {
    id: "turbulence",
    phase: "turbulence",
    intent: "high-frequency multi-object transform/style activity",
  },
  {
    id: "lifecycle-churn",
    phase: "lifecycle-churn",
    intent: "structural presence/lifecycle churn after the morph phases",
  },
]);

export function fixedStressSampleTimes(
  duration = STRESS_DURATION_SECONDS,
  sampleHz = STRESS_SAMPLE_HZ,
) {
  assert.ok(Number.isFinite(duration) && duration > 0, "duration must be positive and finite");
  assert.ok(Number.isSafeInteger(sampleHz) && sampleHz > 0, "sample rate must be positive");
  const count = Math.round(duration * sampleHz);
  assert.equal(count / sampleHz, duration, "duration must end on the fixed sample grid");
  return Array.from({ length: count + 1 }, (_, index) => index / sampleHz);
}

export function classifyStressPhase(time) {
  assert.ok(Number.isFinite(time) && time >= 0, "sample time must be finite and non-negative");
  return STRESS_PHASES.find((phase, index) =>
    time >= phase.start && (time < phase.end || (index === STRESS_PHASES.length - 1 && time <= phase.end)),
  )?.id ?? null;
}

function timingSummary(values) {
  assert.ok(values.length > 0, "performance case must contain at least one sample");
  const sorted = [...values].sort((a, b) => a - b);
  return {
    min: sorted[0],
    max: sorted.at(-1),
    mean: sorted.reduce((sum, value) => sum + value, 0) / sorted.length,
    p95: sorted[Math.ceil(sorted.length * 0.95) - 1],
  };
}

export function summarizeStressPhases(samples) {
  return STRESS_PHASES.map((phase) => {
    const values = samples.filter((sample) => classifyStressPhase(sample.sceneTime) === phase.id)
      .map((sample) => sample.advanceRoundTripMs);
    assert.ok(values.length > 0, `${phase.id} must be sampled`);
    return { ...phase, samples: values.length, advanceRoundTripMs: timingSummary(values) };
  });
}

export function summarizeStressCases(samples) {
  return STRESS_PERFORMANCE_CASES.map((performanceCase) => {
    let selected = samples.filter(
      (sample) => classifyStressPhase(sample.sceneTime) === performanceCase.phase,
    );
    if (performanceCase.takeFirstSamples !== undefined) {
      selected = selected.slice(0, performanceCase.takeFirstSamples);
    }
    if (performanceCase.skipFirstSamples !== undefined) {
      selected = selected.slice(performanceCase.skipFirstSamples);
    }
    assert.ok(selected.length > 0, `${performanceCase.id} must contain measured samples`);
    return {
      ...performanceCase,
      samples: selected.length,
      firstSceneTime: selected[0].sceneTime,
      lastSceneTime: selected.at(-1).sceneTime,
      advanceRoundTripMs: timingSummary(selected.map((sample) => sample.advanceRoundTripMs)),
    };
  });
}

export function validateStressReport(
  report,
  { transportMode, rendererBackend, sampleHz, minimumEffectiveFps = null },
) {
  assert.equal(report.schemaVersion, 2);
  assert.equal(report.execution.mode, "semantic");
  assert.equal(report.execution.transportMode, transportMode);
  assert.equal(report.environment.rendererBackend, rendererBackend);
  assert.equal(report.environment.targetHz, sampleHz);
  assert.equal(report.setup.warmupFrames, 0);
  assert.equal(report.execution.sourceContinuation, true);
  assert.equal(report.execution.sourceCompleted, true);
  assert.equal(report.execution.authoredDuration, STRESS_DURATION_SECONDS);
  assert.equal(report.execution.lastMeasuredTime, STRESS_DURATION_SECONDS);
  assert.equal(report.scene.objects, STRESS_FINAL_VISIBLE_OBJECT_COUNT);
  const times = fixedStressSampleTimes(STRESS_DURATION_SECONDS, sampleHz).slice(1);
  // Time zero is the unmeasured initial presentation. Every subsequent sample
  // must cover the unchanged source on the requested grid through its endpoint.
  assert.equal(report.samples.length, times.length, "complete sample grid is required");
  assert.equal(report.cadence.frames, times.length);
  if (minimumEffectiveFps !== null) {
    assert.ok(
      Number.isFinite(minimumEffectiveFps) && minimumEffectiveFps > 0,
      "minimum effective FPS must be positive and finite",
    );
    const effectiveFps = report.cadence.effective?.effectiveFps;
    assert.ok(
      Number.isFinite(effectiveFps),
      "effective FPS must be available when a minimum is requested",
    );
    assert.ok(
      effectiveFps >= minimumEffectiveFps,
      `effective FPS ${effectiveFps.toFixed(2)} is below required ${minimumEffectiveFps.toFixed(2)}`,
    );
  }
  assert.equal(report.execution.firstMeasuredTime, times[0]);
  report.samples.forEach((sample, index) => {
    assert.ok(Math.abs(sample.sceneTime - times[index]) < 1e-9, `wrong sample time at ${index}`);
    assert.ok(Number.isFinite(sample.advanceRoundTripMs) && sample.advanceRoundTripMs >= 0,
      "advance round trip must be finite and non-negative");
  });
  return summarizeStressPhases(report.samples);
}
