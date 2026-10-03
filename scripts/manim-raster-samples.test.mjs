import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { sampleRasterFrames } from "./manim-raster-support.mjs";

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

  const selectedPaths = [
    direct.source,
    worker.noon_source,
    "crates/noon/src/example_scenes/spatial_surface.rs",
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

test("Line3D and explicit triangular mesh fixtures use the existing paired raster harness", async () => {
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
  assert.equal(direct.expected_object_count, 3);
  assert.equal(worker.expected_object_count, 3);

  const workerSource = await readFile(path.join(repoRoot, worker.noon_source), "utf8");
  assert.match(workerSource, /\bLine3D\(/);
  assert.match(workerSource, /Mesh3D\.polyhedron\(/);
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
  assert.deepEqual(direct.sample_times, [0, 0.5, 0.9666666666666667]);
  assert.deepEqual(worker.sample_times, direct.sample_times);
  assert.deepEqual(worker.raster_tolerance, direct.raster_tolerance);

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
  assert.equal(direct.expected_duration, 3);
  assert.deepEqual(direct.sample_times, [0, 1.5, 2.966666666666667]);
  assert.deepEqual(worker.sample_times, direct.sample_times);
  assert.equal(direct.raster_tolerance, undefined);
  assert.equal(worker.raster_tolerance, undefined);

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
