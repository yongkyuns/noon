import assert from "node:assert/strict";

export const STRESS_DURATION_SECONDS = 5;
export const STRESS_OBJECT_COUNT = 826;
export const STRESS_FINAL_VISIBLE_OBJECT_COUNT = 626;
export const STRESS_TRACK_COUNT = 6889;
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

export function summarizeStressPhases(samples, summarizeSamples) {
  assert.ok(Array.isArray(samples), "samples must be an array");
  assert.equal(typeof summarizeSamples, "function", "sample summarizer is required");
  return STRESS_PHASES.map((phase) => {
    const rows = samples.filter((sample) => classifyStressPhase(sample.sceneTime) === phase.id);
    return {
      ...phase,
      samples: rows.length,
      dirtySamples: rows.filter((sample) => sample.dirty).length,
      engineMs: summarizeSamples(rows.map((sample) => sample.engineMs)),
      transportApplyMs: summarizeSamples(rows.map((sample) => sample.transportApplyMs)),
      rendererRenderMs: summarizeSamples(rows.map((sample) => sample.rendererRenderMs)),
      totalMs: summarizeSamples(rows.map((sample) => sample.totalMs)),
      deltaBytes: summarizeSamples(rows.map((sample) => sample.deltaBytes)),
      uploadBytes: summarizeSamples(rows.map((sample) => sample.uploadBytes)),
      geometryCacheMisses: summarizeSamples(rows.map((sample) => sample.geometryCacheMisses)),
    };
  });
}

export function assertSteadyStressTelemetry(samples, coldFrames = 3) {
  assert.ok(Number.isSafeInteger(coldFrames) && coldFrames >= 0, "cold frame count must be non-negative");
  const steady = samples
    .filter((sample) => classifyStressPhase(sample.sceneTime) === "final-wave")
    .slice(coldFrames);
  assert.ok(steady.length >= 20, "final wave must retain a meaningful steady sample window");
  assert.ok(steady.every((sample) => sample.dirty), "final-wave samples must remain dynamically rendered");
  assert.ok(
    steady.every((sample) => sample.geometryCacheMisses === 0),
    "final-wave geometry cache must be warm after the cold frames",
  );
  assert.ok(
    steady.every((sample) => sample.uploadBytes > 0),
    "dynamic final-wave frames must keep uploading changed instance state",
  );
  const morphs = ["morph-a", "morph-b"].map((phase) => {
    const phaseDefinition = STRESS_PHASES.find((candidate) => candidate.id === phase);
    const rows = samples
      .filter(
        (sample) =>
          classifyStressPhase(sample.sceneTime) === phase &&
          sample.sceneTime > phaseDefinition.start + Number.EPSILON,
      )
      .slice(1);
    assert.ok(rows.length >= 20, `${phase} must retain a meaningful warm sample window`);
    assert.ok(rows.every((sample) => sample.dirty), `${phase} must remain dynamically rendered`);
    assert.ok(
      rows.every((sample) => sample.geometryCacheMisses === 0),
      `${phase} must reuse installed fixed-topology render geometry after its cold frame`,
    );
    assert.ok(
      rows.every((sample) => sample.inlineRenderGeometryCount === 0),
      `${phase} deltas must not resend inline path command streams`,
    );
    assert.ok(
      rows.every((sample) => sample.renderGeometryResourceCount > 0),
      `${phase} deltas must address installed render geometry resources`,
    );
    assert.ok(
      rows.every((sample) => sample.deltaBytes < 512 * 1024),
      `${phase} deltas must stay below the compact retained transport bound`,
    );
    assert.ok(
      rows.every((sample) => sample.uploadBytes > 0 && sample.uploadBytes < 1024 * 1024),
      `${phase} must upload compact dynamic state without rebuilding path vertex/index payloads`,
    );
    return {
      phase,
      coldFrames: 1,
      samples: rows.length,
      maximumGeometryCacheMisses: Math.max(...rows.map((sample) => sample.geometryCacheMisses)),
      maximumInlineRenderGeometryCount: Math.max(
        ...rows.map((sample) => sample.inlineRenderGeometryCount),
      ),
      minimumRenderGeometryResourceCount: Math.min(
        ...rows.map((sample) => sample.renderGeometryResourceCount),
      ),
      maximumDeltaBytes: Math.max(...rows.map((sample) => sample.deltaBytes)),
      minimumUploadBytes: Math.min(...rows.map((sample) => sample.uploadBytes)),
      maximumUploadBytes: Math.max(...rows.map((sample) => sample.uploadBytes)),
    };
  });
  return {
    morphs,
    finalWave: {
      coldFrames,
      samples: steady.length,
      maximumGeometryCacheMisses: Math.max(...steady.map((sample) => sample.geometryCacheMisses)),
      minimumUploadBytes: Math.min(...steady.map((sample) => sample.uploadBytes)),
      maximumUploadBytes: Math.max(...steady.map((sample) => sample.uploadBytes)),
    },
  };
}
