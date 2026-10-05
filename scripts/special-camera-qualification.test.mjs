import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { FOLLOWING_SOURCE, FOLLOWING_TIMES, followingManifest,
  assertFollowingSources, assertFollowingState, assertFollowingReports } from "./special-camera-qualification.mjs";

const paint = (red, green, blue) => ({ red, green, blue, alpha: 1 });
const white = paint(1, 1, 1);
const orange = paint(1, 0.5, 0);
const bounds = { width: 0.16, height: 0.16 };
function referenceFrame(time = 0, index = 0) {
  return { time, frame_index: index, camera: { center: [-2, -1], height: 4 },
    objects: [[[-2, -1], white], [[4, -1], white], [[0, -0.5], orange]].map(([center, fill]) => (
      { type: "Dot", center, bounds: { ...bounds }, fill: { ...fill } })) };
}
function noonFrame(reference = referenceFrame()) {
  return { time: reference.time, camera: structuredClone(reference.camera),
    objects: reference.objects.map((row, i) => ({ ...structuredClone(row), id: 100 + i, present: true })) };
}
function reports() {
  const frames = Array.from({ length: 90 }, (_, i) => referenceFrame(i / 30, i));
  const semantic = { manim_version: "0.21.0", frame_rate: 30,
    fixtures: [{ id: "following-graph-camera", logical_duration: 3, frame_count: 90, frames,
      terminal_state: referenceFrame(3, 90) }] };
  const entry = () => ({ noonDuration: 3, durationDelta: 0,
    samples: FOLLOWING_TIMES.map(time => {
      const index = Math.round(time * 30);
      return { time, frameIndex: index, categories: [], debugFrame: noonFrame(frames[index]) };
    }) });
  const raster = { enforce: true, fixtures: [{ id: "following-graph-camera",
    backends: { webgpu: entry(), webgl: entry() } }] };
  return { raster, semantic };
}

test("focused manifest preserves canonical budgets and covers real frame boundaries", () => {
  const baseline = { reference: { version: "0.21.0", renderer: "cairo", frame_rate: 30,
    pixel_width: 960, pixel_height: 540, source: "old.py" },
    fixtures: [{ id: "unrelated" }], policy: { raster_tolerance: { max_differing_ratio: 0.006 } } };
  const before = structuredClone(baseline);
  const focused = followingManifest(baseline);
  assert.deepEqual(baseline, before);
  assert.deepEqual(focused.policy, baseline.policy);
  assert.equal(focused.fixtures.length, 1);
  assert.equal(focused.fixtures[0].source, FOLLOWING_SOURCE);
  assert.equal(focused.fixtures[0].raster_tolerance, undefined);
  assert.equal(focused.fixtures[0].expected_duration, 3);
  for (const t of FOLLOWING_TIMES) assert.ok(Math.abs(Math.round(t * 30) - t * 30) < 1e-9);
  for (const t of [29 / 30, 1, 31 / 30, 59 / 30, 2, 61 / 30]) assert.ok(FOLLOWING_TIMES.includes(t));
  baseline.reference.frame_rate = 60;
  assert.throws(() => followingManifest(baseline));
});

test("effective camera/dots compare by observable state, not engine identity or order", () => {
  const reference = referenceFrame();
  const actual = noonFrame(reference);
  actual.objects.reverse();
  assert.equal(assertFollowingState(actual, reference).maximumAbsoluteError, 0);
  actual.camera.center[0] += 0.5e-6;
  assert.ok(assertFollowingState(actual, reference).maximumAbsoluteError > 0);
});

for (const [name, mutate] of [
  ["camera tracking", frame => { frame.camera.center[0] += 0.01; }],
  ["camera zoom", frame => { frame.camera.height += 0.01; }],
  ["missing camera", frame => { delete frame.camera; }],
  ["null camera", frame => { frame.camera.height = null; }],
  ["nonfinite camera", frame => { frame.camera.center[0] = NaN; }],
  ["coerced clock", frame => { frame.time = "0"; }],
  ["dot movement", frame => { frame.objects[2].center[0] += 0.01; }],
  ["dot opacity", frame => { frame.objects[2].fill.alpha = 0.9; }],
  ["dot dimensions", frame => { frame.objects[2].bounds.width += 0.00005; }],
  ["dot missing", frame => { frame.objects.pop(); }],
  ["dot hidden", frame => { frame.objects[2].present = false; }],
  ["duplicate dot", frame => { frame.objects[1] = structuredClone(frame.objects[0]); }],
]) test(`rejects ${name} regression`, () => {
  const expected = referenceFrame();
  const actual = noonFrame(expected);
  mutate(actual);
  assert.throws(() => assertFollowingState(actual, expected));
});

test("complete two-backend cohort includes a separate exact terminal observation", () => {
  const { raster, semantic } = reports();
  assert.equal(assertFollowingReports(raster, semantic).checks.length, FOLLOWING_TIMES.length * 2);
});

for (const [name, mutate] of [
  ["report-only raster", ({ raster }) => { raster.enforce = false; }],
  ["missing backend", ({ raster }) => { delete raster.fixtures[0].backends.webgl; }],
  ["missing frame", ({ raster }) => { raster.fixtures[0].backends.webgpu.samples.pop(); }],
  ["reordered frames", ({ raster }) => { raster.fixtures[0].backends.webgpu.samples.reverse(); }],
  ["raster failure", ({ raster }) => { raster.fixtures[0].backends.webgpu.samples[0].categories.push("raster"); }],
  ["duration drift", ({ raster }) => { raster.fixtures[0].backends.webgpu.noonDuration = 3.01; }],
  ["version drift", ({ semantic }) => { semantic.manim_version = "0.19.0"; }],
  ["missing endpoint", ({ semantic }) => { delete semantic.fixtures[0].terminal_state; }],
  ["missing raw frame", ({ semantic }) => { semantic.fixtures[0].frames.pop(); }],
  ["wrong frame index", ({ raster }) => { raster.fixtures[0].backends.webgpu.samples[0].frameIndex = 1; }],
]) test(`qualification rejects ${name}`, () => {
  const evidence = reports();
  mutate(evidence);
  assert.throws(() => assertFollowingReports(evidence.raster, evidence.semantic));
});

test("unchanged class AST is required for both canonical and Noon source", () => {
  const reference = readFileSync(new URL(`../${FOLLOWING_SOURCE}`, import.meta.url), "utf8");
  const noon = readFileSync(new URL("../web/python/examples/manim_example_following_graph_camera.py", import.meta.url), "utf8");
  const body = reference.slice(reference.indexOf("class FollowingGraphCamera"));
  const upstream = `.. manim:: FollowingGraphCamera\n\n${body.split("\n").map(line => `    ${line}`).join("\n")}\n.. manim:: OtherScene\n`;
  assert.match(assertFollowingSources(reference, noon, upstream).upstreamSha256, /^[a-f0-9]{64}$/);
  for (const changed of [
    noon.replace("scale(0.5)", "scale(0.6)"),
    noon.replace("rate_func=linear", "rate_func=smooth"),
    noon.replace("np.sin(x)", "math.sin(x)"),
    `${noon}\nFollowingGraphCamera.construct = lambda self: None\n`,
  ]) assert.throws(() => assertFollowingSources(reference, changed, upstream));
});
