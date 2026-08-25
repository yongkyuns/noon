const POLICY_FIELDS = [
  "max_duration_delta_seconds",
  "max_background_channel_delta_sum",
  "max_bounds_delta_px",
  "max_differing_ratio",
  "max_mean_absolute_channel_error",
];

function assertLimit(name, value, { allowNull = false } = {}) {
  if (allowNull && value === null) return;
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    throw new TypeError(`${name} must be a finite non-negative number${allowNull ? " or null" : ""}`);
  }
}

export function resolveRasterTolerance(manifest, fixture) {
  const defaults = manifest?.policy?.raster_tolerance;
  if (!defaults || typeof defaults !== "object") {
    throw new TypeError("manifest policy.raster_tolerance is required when raster enforcement is enabled");
  }
  const overrides = fixture?.raster_tolerance ?? {};
  const tolerance = { ...defaults, ...overrides };
  for (const field of POLICY_FIELDS) {
    if (!(field in tolerance)) throw new TypeError(`${fixture.id}: missing raster tolerance ${field}`);
    assertLimit(`${fixture.id}.${field}`, tolerance[field], {
      allowNull: field === "max_bounds_delta_px",
    });
  }
  return tolerance;
}

function backgroundDelta(sample) {
  return sample.reference.background
    .map((value, index) => Math.abs(value - sample.noon.background[index]))
    .reduce((sum, value) => sum + value, 0);
}

function boundsDelta(sample) {
  if (!sample.boundsDelta) return 0;
  return Math.max(
    Math.abs(sample.boundsDelta.centroidX),
    Math.abs(sample.boundsDelta.centroidY),
    Math.abs(sample.boundsDelta.width),
    Math.abs(sample.boundsDelta.height),
  );
}

export function evaluateRasterTolerance({ sample, timingDelta, tolerance }) {
  const failures = [];
  const record = (category, metric, actual, limit) => {
    failures.push({ category, metric, actual, limit });
  };

  const durationDelta = Math.abs(timingDelta);
  if (durationDelta > tolerance.max_duration_delta_seconds + 1e-12) {
    record("timing", "duration_delta_seconds", durationDelta, tolerance.max_duration_delta_seconds);
  }

  const bgDelta = backgroundDelta(sample);
  if (bgDelta > tolerance.max_background_channel_delta_sum) {
    record(
      "background/color-pipeline",
      "background_channel_delta_sum",
      bgDelta,
      tolerance.max_background_channel_delta_sum,
    );
  }

  if (tolerance.max_bounds_delta_px !== null) {
    const referenceHasBounds = sample.reference.bounds !== null;
    const noonHasBounds = sample.noon.bounds !== null;
    if (referenceHasBounds !== noonHasBounds) {
      record("camera/layout/geometry", "bounds_presence_mismatch", 1, 0);
    } else {
      const maxBoundsDelta = boundsDelta(sample);
      if (maxBoundsDelta > tolerance.max_bounds_delta_px + 1e-12) {
        record(
          "camera/layout/geometry",
          "bounds_delta_px",
          maxBoundsDelta,
          tolerance.max_bounds_delta_px,
        );
      }
    }
  }

  if (sample.diff.differingRatio > tolerance.max_differing_ratio + 1e-12) {
    record(
      "raster/style/animation-state",
      "differing_ratio",
      sample.diff.differingRatio,
      tolerance.max_differing_ratio,
    );
  }
  if (
    sample.diff.meanAbsoluteChannelError >
    tolerance.max_mean_absolute_channel_error + 1e-12
  ) {
    record(
      "raster/style/animation-state",
      "mean_absolute_channel_error",
      sample.diff.meanAbsoluteChannelError,
      tolerance.max_mean_absolute_channel_error,
    );
  }

  return {
    passed: failures.length === 0,
    categories: [...new Set(failures.map((failure) => failure.category))],
    failures,
  };
}

export function formatRasterPolicyFailure(fixtureId, backend, label, result) {
  return `${fixtureId}/${backend}/${label}: ${result.failures
    .map(
      ({ category, metric, actual, limit }) =>
        `${category} ${metric}=${actual} > ${limit}`,
    )
    .join("; ")}`;
}
