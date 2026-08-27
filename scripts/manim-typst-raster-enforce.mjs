import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const manifest = JSON.parse(
  await readFile(path.join(repoRoot, "parity", "manim-v0.21", "typst-manifest.json"), "utf8"),
);
const artifactRoot = path.resolve(
  repoRoot,
  process.env.NOON_MANIM_TYPST_RASTER_ARTIFACTS ?? "manim-raster-artifacts/retained-typst",
);
const report = JSON.parse(await readFile(path.join(artifactRoot, "report.json"), "utf8"));
const expectedBackends = (process.env.NOON_MANIM_TYPST_RASTER_BACKENDS ?? "webgpu,webgl")
  .split(",")
  .map((value) => value.trim())
  .filter(Boolean);

function finiteNonnegative(value, label) {
  assert.ok(Number.isFinite(value) && value >= 0, `${label} must be finite and nonnegative`);
  return value;
}

function enforceMaximum(actual, maximum, label) {
  assert.ok(Number.isFinite(actual), `${label} must be finite`);
  assert.ok(
    actual <= maximum + Number.EPSILON,
    `${label}: ${actual} exceeds ratchet ${maximum}`,
  );
}

assert.equal(
  manifest.policy?.mode,
  "enforced-per-fixture-ratchet",
  "Typst raster manifest must explicitly enable ratchet enforcement",
);
finiteNonnegative(
  manifest.policy?.max_background_channel_delta_sum,
  "policy.max_background_channel_delta_sum",
);
assert.deepEqual(report.reference, manifest.reference, "Typst raster report/reference drifted from manifest");
assert.ok(Array.isArray(report.fixtures), "Typst raster report must contain fixture results");

const fixturesById = new Map(manifest.fixtures.map((fixture) => [fixture.id, fixture]));
assert.equal(fixturesById.size, manifest.fixtures.length, "Typst fixture IDs must be unique");

for (const fixture of manifest.fixtures) {
  const ratchet = fixture.ratchet;
  assert.ok(ratchet, `${fixture.id}: missing raster ratchet`);
  finiteNonnegative(ratchet.max_differing_ratio, `${fixture.id}.max_differing_ratio`);
  finiteNonnegative(
    ratchet.max_mean_absolute_channel_error,
    `${fixture.id}.max_mean_absolute_channel_error`,
  );
  finiteNonnegative(
    ratchet.max_abs_centroid_x_delta,
    `${fixture.id}.max_abs_centroid_x_delta`,
  );
  finiteNonnegative(
    ratchet.max_abs_centroid_y_delta,
    `${fixture.id}.max_abs_centroid_y_delta`,
  );
  finiteNonnegative(ratchet.max_abs_width_delta, `${fixture.id}.max_abs_width_delta`);
  finiteNonnegative(ratchet.max_abs_height_delta, `${fixture.id}.max_abs_height_delta`);

  for (const backend of expectedBackends) {
    const matches = report.fixtures.filter(
      (result) => result.id === fixture.id && result.backend === backend,
    );
    assert.equal(matches.length, 1, `${backend}/${fixture.id}: expected exactly one raster result`);
    const result = matches[0];
    assert.ok(result.bboxDelta, `${backend}/${fixture.id}: missing bbox delta`);
    assert.equal(
      result.reference.background.length,
      result.actual.background.length,
      `${backend}/${fixture.id}: background channel count mismatch`,
    );
    const backgroundDelta = result.reference.background
      .map((value, index) => Math.abs(value - result.actual.background[index]))
      .reduce((sum, value) => sum + value, 0);

    enforceMaximum(
      backgroundDelta,
      manifest.policy.max_background_channel_delta_sum,
      `${backend}/${fixture.id} background channel delta sum`,
    );
    enforceMaximum(
      result.differingRatio,
      ratchet.max_differing_ratio,
      `${backend}/${fixture.id} differing ratio`,
    );
    enforceMaximum(
      result.meanAbsoluteChannelError,
      ratchet.max_mean_absolute_channel_error,
      `${backend}/${fixture.id} mean absolute channel error`,
    );
    enforceMaximum(
      Math.abs(result.bboxDelta.centroidX),
      ratchet.max_abs_centroid_x_delta,
      `${backend}/${fixture.id} centroid X delta`,
    );
    enforceMaximum(
      Math.abs(result.bboxDelta.centroidY),
      ratchet.max_abs_centroid_y_delta,
      `${backend}/${fixture.id} centroid Y delta`,
    );
    enforceMaximum(
      Math.abs(result.bboxDelta.width),
      ratchet.max_abs_width_delta,
      `${backend}/${fixture.id} width delta`,
    );
    enforceMaximum(
      Math.abs(result.bboxDelta.height),
      ratchet.max_abs_height_delta,
      `${backend}/${fixture.id} height delta`,
    );

    console.log(
      `${backend}/${fixture.id}: ratchet ok ` +
        `diff=${result.differingRatio.toFixed(6)} ` +
        `mae=${result.meanAbsoluteChannelError.toFixed(4)} ` +
        `bbox=${JSON.stringify(result.bboxDelta)}`,
    );
  }
}

const expectedPairs = manifest.fixtures.length * expectedBackends.length;
assert.equal(
  report.fixtures.length,
  expectedPairs,
  `Typst raster report must contain exactly ${expectedPairs} fixture/backend results`,
);
