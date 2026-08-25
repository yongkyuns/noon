import assert from "node:assert/strict";

import {
  evaluateRasterTolerance,
  formatRasterPolicyFailure,
  resolveRasterTolerance,
} from "./manim-raster-policy.mjs";

const manifest = {
  policy: {
    raster_tolerance: {
      max_duration_delta_seconds: 1 / 30 + 1e-6,
      max_background_channel_delta_sum: 0,
      max_bounds_delta_px: 2,
      max_differing_ratio: 0.005,
      max_mean_absolute_channel_error: 0.25,
    },
  },
};
const fixture = { id: "probe" };
const tolerance = resolveRasterTolerance(manifest, fixture);
const baseline = {
  reference: { background: [0, 0, 0, 255], bounds: { minX: 1, minY: 1, maxX: 10, maxY: 10 } },
  noon: { background: [0, 0, 0, 255], bounds: { minX: 1, minY: 1, maxX: 10, maxY: 10 } },
  boundsDelta: { centroidX: 0, centroidY: 0, width: 2, height: -1 },
  diff: { differingRatio: 0.004, meanAbsoluteChannelError: 0.2 },
};

assert.equal(
  evaluateRasterTolerance({ sample: baseline, timingDelta: 0, tolerance }).passed,
  true,
);

function expectCategory(sample, timingDelta, category) {
  const result = evaluateRasterTolerance({ sample, timingDelta, tolerance });
  assert.equal(result.passed, false);
  assert.ok(result.categories.includes(category), `${category} should be enforced`);
  assert.match(
    formatRasterPolicyFailure("probe", "webgpu", "frame-0000", result),
    new RegExp(category.replaceAll("/", "\\/")),
  );
}

expectCategory(baseline, 0.1, "timing");
expectCategory(
  {
    ...baseline,
    noon: { ...baseline.noon, background: [1, 0, 0, 255] },
  },
  0,
  "background/color-pipeline",
);
expectCategory(
  {
    ...baseline,
    boundsDelta: { centroidX: 0, centroidY: 0, width: 3, height: 0 },
  },
  0,
  "camera/layout/geometry",
);
expectCategory(
  {
    ...baseline,
    noon: { ...baseline.noon, bounds: null },
    boundsDelta: null,
  },
  0,
  "camera/layout/geometry",
);
expectCategory(
  {
    ...baseline,
    diff: { differingRatio: 0.02, meanAbsoluteChannelError: 0.2 },
  },
  0,
  "raster/style/animation-state",
);
expectCategory(
  {
    ...baseline,
    diff: { differingRatio: 0.004, meanAbsoluteChannelError: 1.0 },
  },
  0,
  "raster/style/animation-state",
);

const boundsExempt = resolveRasterTolerance(manifest, {
  id: "known-geometry-gap",
  raster_tolerance: { max_bounds_delta_px: null },
});
assert.equal(
  evaluateRasterTolerance({
    sample: {
      ...baseline,
      noon: { ...baseline.noon, bounds: null },
      boundsDelta: null,
    },
    timingDelta: 0,
    tolerance: boundsExempt,
  }).passed,
  true,
);

assert.throws(
  () =>
    resolveRasterTolerance(
      {
        policy: {
          raster_tolerance: {
            ...manifest.policy.raster_tolerance,
            max_differing_ratio: -1,
          },
        },
      },
      fixture,
    ),
  /finite non-negative number/,
);

console.log("Manim raster ratchet policy tests passed");
