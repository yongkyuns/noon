import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { rasterFixtureSource } from "./manim-raster-support.mjs";

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
const fullManifest = { reference: oracle, fixtures: [matching, other] };
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
    "assert", "baseline", "reference", "path", "repoRoot", "manifest", "readFile", "rasterFixtureSource", "page", "fixture", "baseUrl",
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
  const dependencies = [assert, baseline, reference, path, repoRoot, fullManifest,
    async () => "from manim import *\nclass Example(Scene): pass\n", rasterFixtureSource, page];
  const plain = { id: "plain", scene: "Example", expected_duration: 1 };
  const latex = { ...plain, id: "latex", requires_latex: true };
  for (const fixture of [plain, latex]) {
    baseline.fixtures = [{ ...fixture, expectedDuration: 1, backends: { webgpu: { samples: [{}] } } }];
    reference.fixtures = [{ id: fixture.id, frame_count: 1, frames: [{ time: 0 }] }];
    await assert.rejects(qualify(...dependencies, fixture, "http://example.test"), stopAfterLoad);
  }
  assert.equal(loadedSources.length, 2);
  assert.doesNotMatch(loadedSources[0], /await prepare_latex\(\)/);
  assert.match(loadedSources[1], /await prepare_latex\(\)/);
});
