import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { sampleRasterFrames } from "./manim-raster-support.mjs";
import { resolveRasterTolerance } from "./manim-raster-policy.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const referenceTimes = Array.from({ length: 66 }, (_, index) => index / 30);
const fractions = [0, 0.25, 0.5, 0.75, 1];

test("fraction sampling retains the existing rounded-frame contract", () => {
  const samples = sampleRasterFrames(referenceTimes, fractions);
  assert.deepEqual(samples.map(({ frameIndex }) => frameIndex), [0, 16, 33, 49, 65]);
  assert.equal(samples.at(-1).label, "frame-0065");
});

test("matching phases and separate frozen holds use exact reference frames", () => {
  const times = [0, 0.5, 1, 1.5, 2, 2.1];
  // Cairo materializes sixty animation PNGs followed by one PNG per static wait.
  const materialized = [...Array.from({ length: 60 }, (_, i) => i / 30), 2, 2.1];
  const samples = sampleRasterFrames(materialized, fractions, times);
  assert.deepEqual(samples.map(({ frameIndex }) => frameIndex), [0, 15, 30, 45, 60, 61]);
  assert.deepEqual(samples.map(({ time }) => time), times);
});

test("one frozen hold cannot supply a fabricated post-cleanup frame", () => {
  const materialized = [...Array.from({ length: 60 }, (_, i) => i / 30), 2];
  assert.throws(() => sampleRasterFrames(materialized, fractions, [2, 2.1]),
    /no reference frame at requested logical time 2.1/);
});

test("a missing contract boundary fails instead of selecting a nearby frame", () => {
  assert.throws(() => sampleRasterFrames(referenceTimes, fractions, [0.25]),
    /no reference frame/);
});

test("clock roundoff still returns the actual reference timestamp", () => {
  const samples = sampleRasterFrames([0, 0.3 + 1e-12], fractions, [0, 0.3]);
  assert.equal(samples[1].time, 0.3 + 1e-12);
});

test("malformed explicit times cannot silently fall back to fractions", () => {
  for (const times of [null, [], [NaN], [-1], [1, 0], [0, 0]]) {
    assert.throws(() => sampleRasterFrames(referenceTimes, fractions, times));
  }
});

test("invalid or backwards reference clocks are rejected", () => {
  for (const times of [[], [NaN], [-1], [0, 0.5, 0.25]]) {
    assert.throws(() => sampleRasterFrames(times, fractions));
  }
});

test("fraction samples still collapse duplicate frame indices", () => {
  assert.deepEqual(sampleRasterFrames([0], fractions), [
    { frameIndex: 0, time: 0, label: "frame-0000" },
  ]);
});

test("invalid fractions are rejected", () => {
  for (const values of [[], [-0.1], [1.1], [NaN]]) {
    assert.throws(() => sampleRasterFrames(referenceTimes, values));
  }
});

test("spatial surface fixtures and their sources trigger raster qualification", async () => {
  const manifest = JSON.parse(await readFile(
    path.join(repoRoot, "parity/manim-v0.21/manifest.json"), "utf8",
  ));
  const workflow = await readFile(
    path.join(repoRoot, ".github/workflows/manim-raster-differential.yml"), "utf8",
  );
  const direct = manifest.fixtures.find(fixture => fixture.id === "spatial-surface-direct");
  const worker = manifest.fixtures.find(fixture => fixture.id === "spatial-surface-worker");
  assert.ok(direct, "native/direct-WASM surface fixture is selected by the raster manifest");
  assert.ok(worker, "Python-worker surface fixture is selected by the raster manifest");
  assert.equal(direct.source, "parity/manim-v0.21/spatial_surface.py");
  assert.equal(worker.source, direct.source);
  assert.equal(worker.noon_source, "web/python/examples/noon_spatial_surface.py");
  const workerSource = await readFile(path.join(repoRoot, worker.noon_source), "utf8");
  assert.match(workerSource, /\bSurface\(/,
    "the worker raster fixture exercises the public bounded Surface adapter");
  assert.match(workerSource, /checkerboard_colors=False[\s\S]*stroke_width=0[\s\S]*shade_in_3d=False/,
    "the fixture opts into geometry/material settings that the retained mesh lane supports");

  const cairoDirect = manifest.fixtures.find(fixture => fixture.id === "spatial-surface-cairo-direct");
  const cairoWorker = manifest.fixtures.find(fixture => fixture.id === "spatial-surface-cairo-worker");
  assert.ok(cairoDirect?.direct_factory, "default Cairo Surface uses direct typed execution");
  assert.ok(cairoWorker?.noon_source, "default Cairo Surface uses the Python worker");
  assert.equal(cairoDirect.source, direct.source);
  assert.equal(cairoWorker.source, direct.source);
  assert.equal(cairoWorker.noon_source, worker.noon_source);
  assert.equal(cairoDirect.scene, "CairoSpatialSurface");
  assert.equal(cairoWorker.scene, cairoDirect.scene);
  assert.equal(cairoDirect.expected_duration, 1);
  assert.equal(cairoWorker.expected_duration, 1);
  assert.deepEqual(cairoDirect.sample_times, [0, 0.5, 29 / 30]);
  assert.deepEqual(cairoWorker.sample_times, cairoDirect.sample_times);
  assert.equal(cairoDirect.expected_object_count, 65);
  assert.equal(cairoWorker.expected_object_count, 65);
  const cairoReference = await readFile(path.join(repoRoot, cairoDirect.source), "utf8");
  const cairoWorkerSource = await readFile(path.join(repoRoot, cairoWorker.noon_source), "utf8");
  const nativeSource = await readFile(
    path.join(repoRoot, "crates/noon/src/example_scenes/spatial_surface.rs"), "utf8",
  );
  assert.match(nativeSource, /UvSurfacePlan::new\(\[-1\.0, 1\.0\], \[-1\.0, 1\.0\], \[8, 8\]\)/,
    "direct Rust samples the same bounded 8×8 UV domain");
  assert.match(nativeSource, /sample_cairo\(\|u, v\|\s*SemanticVec3::new\(u, v, 0\.35 \* \(u \* u \+ v \* v\)\)\s*\)/,
    "direct Rust uses the same nonlinear Cairo control-point callback");
  assert.match(nativeSource, /material: SemanticSpatialMaterial::CairoSurface/,
    "direct Rust selects the distinct Cairo Surface material");
  for (const [label, source] of [["reference", cairoReference], ["worker", cairoWorkerSource]]) {
    const fixture = source.split("class CairoSpatialSurface", 2)[1];
    assert.ok(fixture, `${label}: missing CairoSpatialSurface`);
    assert.match(fixture, /Surface\([\s\S]*?u_range=\(-1, 1\)[\s\S]*?v_range=\(-1, 1\)[\s\S]*?resolution=\(8, 8\)/,
      `${label}: bounded Surface dimensions match the direct Rust case`);
    assert.match(fixture, /0\.35\s*\*\s*\(u\s*\*\s*u\s*\+\s*v\s*\*\s*v\)/,
      `${label}: callback matches the nonlinear capability fixture`);
    assert.doesNotMatch(fixture, /shade_in_3d\s*=\s*False|checkerboard_colors\s*=|stroke_width\s*=/,
      `${label}: uses pinned Surface defaults for shading, checkerboard, and border`);
    assert.match(fixture, /wait\(1\)/,
      `${label}: holds the static scene for the one-second fixture duration`);
  }

  const selectedPaths = [
    direct.source,
    worker.noon_source,
    "crates/noon/src/example_scenes/spatial_surface.rs",
    "crates/noon-web/src/direct_execution_smoke.rs",
    "crates/noon-native/examples/spatial_surface.rs",
    "crates/noon-geometry/src/surface.rs",
    "crates/noon-geometry/src/solids.rs",
    "crates/noon-core/src/spatial3d.rs",
    "crates/noon-core/src/resources/mesh.rs",
    "web/python/_noon_spatial.py",
    "web/python/_manim_spatial_geometry.py",
    "web/python/test_noon_spatial.py",
  ];
  for (const relativePath of selectedPaths) {
    await readFile(path.join(repoRoot, relativePath));
    assert.ok(workflow.includes(`"${relativePath}"`),
      `${relativePath} must trigger the existing raster CI workflow`);
  }
});

test("Line3D, triangle, and translucent Prism fixtures use the existing paired raster harness", async () => {
  const manifest = JSON.parse(await readFile(
    path.join(repoRoot, "parity/manim-v0.21/manifest.json"), "utf8",
  ));
  const workflow = await readFile(
    path.join(repoRoot, ".github/workflows/manim-raster-differential.yml"), "utf8",
  );
  const direct = manifest.fixtures.find(fixture => fixture.id === "spatial-primitives-direct");
  const worker = manifest.fixtures.find(fixture => fixture.id === "spatial-primitives-worker");
  assert.ok(direct?.direct_factory, "native/direct-WASM spatial-primitives fixture is registered");
  assert.ok(worker?.noon_source, "Python-worker spatial-primitives fixture is registered");
  assert.equal(direct.source, "parity/manim-v0.21/spatial_primitives.py");
  assert.equal(worker.source, direct.source);
  assert.equal(worker.noon_source, "web/python/examples/noon_spatial_primitives.py");
  assert.equal(direct.expected_object_count, 14);
  assert.equal(worker.expected_object_count, 14);

  const workerSource = await readFile(path.join(repoRoot, worker.noon_source), "utf8");
  assert.match(workerSource, /\bLine3D\(/);
  assert.match(workerSource, /Prism\(/);
  assert.match(workerSource, /fill_opacity=0\.75/);
  assert.match(workerSource, /show_ends=False/);
  assert.match(workerSource, /show_base=False/);
  assert.match(workerSource, /u_range=\(PI \/ 4, 3 \* PI \/ 4\), v_range=\(PI \/ 6, 5 \* PI \/ 6\)/);
  assert.match(workerSource, /checkerboard_colors=False[\s\S]*stroke_width=0[\s\S]*shade_in_3d=False/,
    "public Line3D selects geometry, opacity, stroke, and shading defaults supported by its mesh profile");
  for (const relativePath of [
    direct.source,
    worker.noon_source,
    "crates/noon/src/example_scenes/spatial_primitives.rs",
    "crates/noon-native/examples/spatial_primitives.rs",
    "crates/noon-web/src/direct_execution_smoke.rs",
    "web/python/_manim_spatial_geometry.py",
    "web/python/_noon_spatial.py",
  ]) {
    await readFile(path.join(repoRoot, relativePath));
    assert.ok(workflow.includes(`"${relativePath}"`),
      `${relativePath} must trigger the existing raster CI workflow`);
  }
});

test("mixed camera-label fixtures enroll the direct and Python worker sources", async () => {
  const manifest = JSON.parse(await readFile(
    path.join(repoRoot, "parity/manim-v0.21/manifest.json"), "utf8",
  ));
  const workflow = await readFile(
    path.join(repoRoot, ".github/workflows/manim-raster-differential.yml"), "utf8",
  );
  const direct = manifest.fixtures.find(fixture => fixture.id === "spatial-camera-labels-direct");
  const worker = manifest.fixtures.find(fixture => fixture.id === "spatial-camera-labels-worker");
  assert.ok(direct?.direct_factory, "typed Rust/WASM camera-label fixture is registered");
  assert.ok(worker?.noon_source, "async Python camera-label fixture is registered");
  assert.equal(direct.source, "parity/manim-v0.21/spatial_camera_labels.py");
  assert.equal(worker.source, direct.source);
  assert.equal(worker.noon_source, "web/python/examples/noon_spatial_camera_labels.py");
  assert.equal(direct.expected_object_count, 7);
  assert.equal(worker.expected_object_count, 7);

  for (const relativePath of [
    direct.source,
    worker.noon_source,
    "crates/noon/src/example_scenes/spatial_camera_labels.rs",
    "crates/noon-native/examples/spatial_camera_labels.rs",
    "crates/noon-web/src/direct_execution_smoke.rs",
  ]) {
    await readFile(path.join(repoRoot, relativePath));
    assert.ok(workflow.includes(`"${relativePath}"`),
      `${relativePath} must trigger the existing raster CI workflow`);
  }
});

test("ThreeDAxes direct and worker fixtures enroll source, timing, and Rust coordinate coverage", async () => {
  const manifest = JSON.parse(await readFile(
    path.join(repoRoot, "parity/manim-v0.21/manifest.json"), "utf8",
  ));
  const workflow = await readFile(
    path.join(repoRoot, ".github/workflows/manim-raster-differential.yml"), "utf8",
  );
  const direct = manifest.fixtures.find(fixture => fixture.id === "spatial-three-d-axes-direct");
  const worker = manifest.fixtures.find(fixture => fixture.id === "spatial-three-d-axes-worker");
  assert.equal(direct?.direct_factory, "createDirectSpatialThreeDAxesSmokeRenderer");
  assert.equal(worker?.noon_source, "web/python/examples/noon_spatial_three_d_axes.py");
  assert.equal(direct.source, "parity/manim-v0.21/spatial_three_d_axes.py");
  assert.equal(direct.expected_object_count, 38);
  assert.equal(worker.expected_object_count, 38);
  assert.deepEqual(direct.sample_times, [0, 0.5, 0.9666666666666667]);
  assert.deepEqual(worker.sample_times, direct.sample_times);
  assert.deepEqual(worker.raster_tolerance, direct.raster_tolerance);

  const nativeSource = await readFile(path.join(repoRoot, "crates/noon/src/example_scenes/spatial_three_d_axes.rs"), "utf8");
  const referenceSource = await readFile(path.join(repoRoot, direct.source), "utf8");
  const workerSource = await readFile(path.join(repoRoot, worker.noon_source), "utf8");
  assert.match(nativeSource, /axis_overrides\[0\]\.tips = Some\(false\)/);
  assert.match(nativeSource, /tipless X axis keeps both endpoint ticks/);
  assert.match(referenceSource, /include_tip": False/);
  assert.match(workerSource, /include_tip": False/);
  assert.match(nativeSource, /stroke_width = Some\(0\.04\)/);
  assert.match(referenceSource, /stroke_width": 4/);
  assert.match(workerSource, /stroke_width": 4/);
  assert.match(referenceSource, /z_axis_config=\{"color": BLUE\}/);
  assert.match(workerSource, /z_axis_config=\{"color": BLUE\}/);
  assert.match(referenceSource, /axes\.get_axis_labels\(/);
  assert.match(workerSource, /axes\.get_axis_labels\(/);
  assert.match(nativeSource, /create_axis_label_targets/);
  for (const source of [nativeSource, referenceSource, workerSource]) {
    assert.match(source, /tick_size.*0\.15|tick_size: Some\(0\.15\)/);
    assert.match(source, /RED/);
    assert.match(source, /GREEN/);
    assert.match(source, /BLUE/);
  }

  for (const relativePath of [
    direct.source,
    worker.noon_source,
    "crates/noon/src/example_scenes/spatial_three_d_axes.rs",
    "crates/noon/src/coordinate_authoring/three_d_axes.rs",
    "crates/noon-web/src/direct_execution_smoke.rs",
  ]) {
    await readFile(path.join(repoRoot, relativePath));
    assert.ok(workflow.includes(`"${relativePath}"`),
      `${relativePath} must trigger the existing raster CI workflow`);
  }
});

test("VectorScene/LTS matrix fixture pairs native and Python ordinary timelines", async () => {
  const manifest = JSON.parse(await readFile(
    path.join(repoRoot, "parity/manim-v0.21/manifest.json"), "utf8",
  ));
  const workflow = await readFile(
    path.join(repoRoot, ".github/workflows/manim-raster-differential.yml"), "utf8",
  );
  const direct = manifest.fixtures.find(fixture => fixture.id === "vector-space-lts-direct");
  const worker = manifest.fixtures.find(fixture => fixture.id === "vector-space-lts-worker");
  assert.equal(direct?.direct_factory, "createDirectVectorSpaceSmokeRenderer");
  assert.equal(worker?.noon_source, "web/python/examples/noon_vector_space.py");
  assert.equal(direct.source, "parity/manim-v0.21/vector_space.py");
  assert.equal(worker.source, direct.source);
  assert.equal(direct.expected_duration, 4);
  assert.deepEqual(direct.sample_times, [0, 0.5, 1, 2.5, 3.966666666666667]);
  assert.deepEqual(worker.sample_times, direct.sample_times);
  assert.deepEqual(direct.raster_tolerance, {
    max_bounds_delta_px: 1, max_differing_ratio: 0.04,
  });
  assert.deepEqual(worker.raster_tolerance, direct.raster_tolerance);
  const tolerance = resolveRasterTolerance(manifest, direct);
  assert.equal(tolerance.max_mean_absolute_channel_error, 0.5);
  assert.equal(tolerance.max_background_channel_delta_sum, 0);

  for (const relativePath of [
    direct.source,
    worker.noon_source,
    "crates/noon/src/example_scenes/vector_space.rs",
    "crates/noon/src/vector_space_authoring.rs",
    "crates/noon-native/examples/vector_space.rs",
    "crates/noon-web/src/direct_execution_smoke.rs",
    "web/python/_manim_vector_space.py",
    "web/python/test_manim_vector_space.py",
  ]) {
    await readFile(path.join(repoRoot, relativePath));
    assert.ok(workflow.includes(`"${relativePath}"`),
      `${relativePath} must trigger the existing raster workflow`);
  }
});

test("focused LTS feature slice pairs native and Python coordinate and ghost behavior", async () => {
  const manifest = JSON.parse(await readFile(
    path.join(repoRoot, "parity/manim-v0.21/manifest.json"), "utf8",
  ));
  const workflow = await readFile(
    path.join(repoRoot, ".github/workflows/manim-raster-differential.yml"), "utf8",
  );
  const direct = manifest.fixtures.find(fixture => fixture.id === "lts-feature-slice-direct");
  const worker = manifest.fixtures.find(fixture => fixture.id === "lts-feature-slice-worker");
  assert.equal(direct?.direct_factory, "createDirectVectorSpaceFeaturesRenderer");
  assert.equal(worker?.noon_source, "web/python/examples/noon_vector_space_features.py");
  assert.equal(direct.source, "parity/manim-v0.21/vector_space_features.py");
  assert.equal(worker.source, direct.source);
  assert.equal(direct.expected_duration, 1.5);
  assert.deepEqual(worker.sample_times, direct.sample_times);
  for (const relativePath of [
    direct.source,
    worker.noon_source,
    "crates/noon/src/example_scenes/vector_space_features.rs",
    "crates/noon-web/src/direct_execution_smoke.rs",
    "web/python/_manim_vector_space.py",
    "web/python/_manim_number_plane.py",
    "web/python/_manim_number_labels.py",
  ]) {
    await readFile(path.join(repoRoot, relativePath));
    assert.ok(workflow.includes(`"${relativePath}"`),
      `${relativePath} must trigger the existing raster CI workflow`);
  }
});
