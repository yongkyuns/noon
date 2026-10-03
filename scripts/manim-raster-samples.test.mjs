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
    "web/python/test_noon_spatial.py",
  ];
  for (const relativePath of selectedPaths) {
    await readFile(path.join(repoRoot, relativePath));
    assert.ok(workflow.includes(`"${relativePath}"`),
      `${relativePath} must trigger the existing raster CI workflow`);
  }
});
