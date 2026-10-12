import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { rasterFixtureSource, resolveSharedPlaybackSamples } from "./manim-raster-support.mjs";

const source = await readFile(new URL("./shared-playback-raster.mjs", import.meta.url), "utf8");
// Execute the real configuration boundary, without importing Playwright or
// starting a server. Only file reads and environment variables are substituted.
const start = source.indexOf("const manifest");
const end = source.indexOf("// The corpus callbacks", start);
assert.ok(start >= 0 && end > start, "playback configuration boundary moved");
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const configure = new AsyncFunction("assert", "readFile", "path", "repoRoot", "process",
  `${source.slice(start, end)}\nreturn { manifest, fixtures, baseline, reference, backends };`);
const repoRoot = path.resolve("playback-test-repository");
const oracle = { version: "0.21.0", frame_rate: 30, source: "fixture.py" };
const matching = { id: "matching-shapes-reordered", scene: "MatchingShapesReordered", expected_duration: 2.2 };
const other = { id: "other", scene: "Other", expected_duration: 1 };
const direct = { id: "direct-rust", scene: "RustFixture", expected_duration: 1,
  direct_factory: "createDirectSpatialMeshSmokeRenderer" };
const fullManifest = { reference: oracle, fixtures: [matching, other, direct] };
const focusedManifest = { reference: oracle, fixtures: [matching] };
const focusedPath = path.resolve("outside-repository", "focused.json");

async function load(env = {}, baselineReference = oracle) {
  const files = new Map([
    [path.join(repoRoot, "parity/manim-v0.21/manifest.json"), fullManifest],
    [focusedPath, focusedManifest],
    [path.join(repoRoot, "focused.json"), focusedManifest],
    [path.join(repoRoot, "manim-raster-artifacts/report.json"),
      { reference: baselineReference, fixtures: [{ ...matching, expectedDuration: matching.expected_duration }] }],
    [path.join(repoRoot, "manim-raster-artifacts/semantic/manim-all-frames.json"),
      { manim_version: oracle.version, frame_rate: oracle.frame_rate }],
  ]);
  const reads = [];
  const result = await configure(assert, async (filename, encoding) => {
    assert.equal(encoding, "utf8");
    reads.push(filename);
    if (!files.has(filename)) throw Object.assign(new Error(`ENOENT: ${filename}`), { code: "ENOENT" });
    return JSON.stringify(files.get(filename));
  }, path, repoRoot, { env });
  return { ...result, reads };
}

test("default manifest retains the full corpus, even with a focused baseline", async () => {
  const result = await load();
  assert.deepEqual(result.fixtures, [matching, other]);
  assert.deepEqual(result.backends, ["webgpu", "webgl"]);
});

test("Python cadence pass excludes direct factories instead of executing their Manim reference source", async () => {
  const result = await load();
  assert.ok(result.fixtures.every(fixture => !fixture.direct_factory));
  await assert.rejects(load({ NOON_SHARED_PLAYBACK_FIXTURES: direct.id }), /unknown playback fixture/);
});

test("absolute focused manifest selects exactly the dense-run fixture", async () => {
  const result = await load({ NOON_MANIM_RASTER_MANIFEST: focusedPath });
  assert.deepEqual(result.fixtures, [matching]);
  assert.equal(result.reads[0], focusedPath);
});

test("relative focused manifest resolves against the repository root", async () => {
  const result = await load({ NOON_MANIM_RASTER_MANIFEST: "focused.json" });
  assert.deepEqual(result.fixtures, [matching]);
  assert.equal(result.reads[0], path.join(repoRoot, "focused.json"));
});

test("explicit playback subset still works within the chosen manifest", async () => {
  const result = await load({ NOON_MANIM_RASTER_MANIFEST: focusedPath,
    NOON_SHARED_PLAYBACK_FIXTURES: matching.id });
  assert.deepEqual(result.fixtures, [matching]);
});

test("playback selection cannot escape the focused manifest", async () => {
  await assert.rejects(load({ NOON_MANIM_RASTER_MANIFEST: focusedPath,
    NOON_SHARED_PLAYBACK_FIXTURES: other.id }), /unknown playback fixture/);
});

test("missing override fails instead of falling back to the full manifest", async () => {
  await assert.rejects(load({ NOON_MANIM_RASTER_MANIFEST: "missing.json" }), { code: "ENOENT" });
});

test("focused manifest retains the existing oracle-identity assertion", async () => {
  await assert.rejects(load({ NOON_MANIM_RASTER_MANIFEST: focusedPath },
    { ...oracle, frame_rate: 60 }), /dense raster reference configuration changed/);
});

test("sparse playback maps rounded logical samples through canonical materialized frame times", () => {
  const fixture = { id: "rounded", expected_duration: 1.5,
    sample_times: [0.5166666666666666, 1.4666666666666666] };
  const semanticFixture = {
    frame_count: 3,
    frames: [{ time: 0 }, { time: 0.5166666666666668 }, { time: 1.4666666666666668 }],
    frozen_intervals: [],
  };
  const samples = resolveSharedPlaybackSamples({
    fixture,
    semanticFixture,
    pngFrameCount: 3,
    denseSamples: [
      { time: fixture.sample_times[0], materializedTime: semanticFixture.frames[1].time,
        frameIndex: 1, referenceKind: "sequence" },
      { time: fixture.sample_times[1], materializedTime: semanticFixture.frames[2].time,
        frameIndex: 2, referenceKind: "sequence" },
    ],
  });
  assert.deepEqual(samples.map(({ frameIndex, time, materializedTime, label }) =>
    [frameIndex, time, materializedTime, label]), [
    [1, fixture.sample_times[0], semanticFixture.frames[1].time, "frame-0001"],
    [2, fixture.sample_times[1], semanticFixture.frames[2].time, "frame-0002"],
  ]);
});

test("sparse playback preserves terminal labels and rejects dense samples outside the resolver contract", () => {
  const fixture = { id: "terminal", expected_duration: 0, sample_times: [0] };
  const semanticFixture = { frame_count: 0, frames: [], frozen_intervals: [],
    terminal_state: { time: 0 }, terminal_png: { path: "terminal.png" } };
  const resolved = resolveSharedPlaybackSamples({
    fixture, semanticFixture, pngFrameCount: 0,
    denseSamples: [{ time: 0, materializedTime: 0, frameIndex: null, referenceKind: "terminal" }],
  });
  assert.equal(resolved[0].label, "frame-0000-terminal");
  assert.throws(() => resolveSharedPlaybackSamples({
    fixture, semanticFixture, pngFrameCount: 0,
    denseSamples: [{ time: 0, materializedTime: 0, frameIndex: 0, referenceKind: "sequence" }],
  }), /dense sample 0 disagrees with canonical terminal resolution/);
});

test("sparse playback resolves held logical checkpoints to their recorded reference frame", () => {
  const fixture = { id: "hold", expected_duration: 2, sample_times: [1.25] };
  const semanticFixture = {
    frame_count: 3,
    frames: [{ time: 0 }, { time: 0.5 }, { time: 1 }],
    frozen_intervals: [{ frame_index: 2, start_time: 1, end_time: 1.5 }],
  };
  const [sample] = resolveSharedPlaybackSamples({
    fixture, semanticFixture, pngFrameCount: 3,
    denseSamples: [{ time: 1.25, materializedTime: 1, frameIndex: 2, referenceKind: "frozen-hold" }],
  });
  assert.deepEqual([sample.label, sample.frameIndex, sample.time, sample.materializedTime],
    ["frame-0002-hold-1_25", 2, 1.25, 1]);
  assert.throws(() => resolveSharedPlaybackSamples({
    fixture, semanticFixture: { ...semanticFixture, frozen_intervals: [] }, pngFrameCount: 3,
    denseSamples: [{ time: 1.25, materializedTime: 1, frameIndex: 2, referenceKind: "frozen-hold" }],
  }), /no reference frame at requested logical time 1\.25/);
});

test("selected fixtures without dense evidence still fail rather than being skipped", async () => {
  const result = await load();
  const functionStart = source.indexOf("async function qualifyFixture(");
  const functionEnd = source.indexOf("\nconst results =", functionStart);
  assert.ok(functionStart >= 0 && functionEnd > functionStart, "playback qualification boundary moved");
  const qualify = new AsyncFunction("assert", "baseline", "fixture",
    `${source.slice(functionStart, functionEnd)}\nreturn qualifyFixture(null, fixture, "webgpu");`);
  await assert.rejects(qualify(assert, result.baseline, other), /dense fixture source selection changed/);
});

test("shared playback passes fixture preparation metadata through the canonical source adapter", async () => {
  const functionStart = source.indexOf("async function qualifyFixture(");
  const functionEnd = source.indexOf("\nconst results =", functionStart);
  assert.ok(functionStart >= 0 && functionEnd > functionStart, "playback qualification boundary moved");
  const qualify = new AsyncFunction(
    "assert", "baseline", "reference", "path", "repoRoot", "manifest", "readFile", "rasterFixtureSource", "resolveSharedPlaybackSamples", "page", "fixture", "baseUrl",
    `${source.slice(functionStart, functionEnd)}\nreturn qualifyFixture(page, fixture, "webgpu");`,
  );
  const loadedSources = [];
  const stopAfterLoad = new Error("stop after shared source adaptation");
  const page = {
    async goto() {},
    async waitForFunction() {},
    async evaluate(_callback, argument) {
      if (argument?.source !== undefined) {
        loadedSources.push(argument.source);
        throw stopAfterLoad;
      }
      return true;
    },
  };
  const baseline = { fixtures: [] };
  const reference = { fixtures: [] };
  const sourceReads = [];
  const dependencies = [assert, baseline, reference, path, repoRoot, fullManifest,
    async filename => {
      sourceReads.push(filename);
      return "from manim import *\nclass Example(Scene): pass\n";
    }, rasterFixtureSource, () => [{ time: 0, frameIndex: 0, referenceKind: "sequence", label: "frame-0000" }], page];
  const plain = { id: "plain", scene: "Example", expected_duration: 1 };
  const latex = { ...plain, id: "latex", requires_latex: true };
  const worker = { ...plain, id: "worker", source: "reference.py", noon_source: "worker.py" };
  for (const fixture of [plain, latex, worker]) {
    baseline.fixtures = [{ ...fixture, expectedDuration: 1, manim: { frameCount: 1 },
      backends: { webgpu: { samples: [{}] } } }];
    reference.fixtures = [{ id: fixture.id, frame_count: 1, frames: [{ time: 0 }] }];
    await assert.rejects(qualify(...dependencies, fixture, "http://example.test"), stopAfterLoad);
  }
  assert.equal(loadedSources.length, 3);
  assert.equal(sourceReads[2], path.join(repoRoot, "worker.py"));
  assert.doesNotMatch(loadedSources[0], /await prepare_latex\(\)/);
  assert.match(loadedSources[1], /await prepare_latex\(\)/);
});
