import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";

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
const oracle = { version: "0.21.0", frame_rate: 30 };
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
