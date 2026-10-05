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
function manifest() {
  return followingManifest({ reference: { version: "0.21.0", renderer: "cairo", frame_rate: 30,
    pixel_width: 960, pixel_height: 540, source: "old.py" }, policy: { raster_tolerance: {
      max_duration_delta_seconds: 0.034, max_background_channel_delta_sum: 0,
      max_bounds_delta_px: 2, max_differing_ratio: 0.006, max_mean_absolute_channel_error: 0.5,
    } } });
}
function rasterSample() {
  const image = { width: 960, height: 540, background: [0, 0, 0, 255], changedPixels: 1000,
    bounds: { minX: 20, minY: 20, maxX: 100, maxY: 100 } };
  return { reference: structuredClone(image), noon: structuredClone(image),
    boundsDelta: { centroidX: 0, centroidY: 0, width: 0, height: 0 },
    diff: { differingPixels: 0, differingRatio: 0, meanAbsoluteChannelError: 0 } };
}
function reports() {
  const frames = Array.from({ length: 90 }, (_, i) => referenceFrame(i / 30, i));
  const semantic = { manim_version: "0.21.0", frame_rate: 30,
    fixtures: [{ id: "following-graph-camera", logical_duration: 3, frame_count: 90, frames,
      terminal_state: referenceFrame(3, 90) }] };
  const entry = () => ({ noonDuration: 3, durationDelta: 0, tolerance: manifest().policy.raster_tolerance,
    samples: FOLLOWING_TIMES.map(time => {
      const index = Math.round(time * 30);
      return { time, frameIndex: index, categories: [], debugFrame: noonFrame(frames[index]), ...rasterSample() };
    }) });
  const raster = { enforce: true, reference: manifest().reference, fixtures: [{ id: "following-graph-camera",
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
  assert.equal(assertFollowingReports(raster, semantic, manifest()).checks.length, FOLLOWING_TIMES.length * 2);
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
  assert.throws(() => assertFollowingReports(evidence.raster, evidence.semantic, manifest()));
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

// Pass labels cannot substitute for raw finite measurements and the actual
// manifest policy. These reports intentionally keep their categories empty.
for (const [name, mutate] of [
  ["100 percent raster difference", sample => { sample.diff.differingPixels = 960 * 540; sample.diff.differingRatio = 1; }],
  ["excess mean channel error", sample => { sample.diff.meanAbsoluteChannelError = 2; }],
  ["missing raw metrics", sample => { delete sample.diff; }],
  ["missing ratio", sample => { delete sample.diff.differingRatio; }],
  ["NaN ratio", sample => { sample.diff.differingRatio = NaN; }],
  ["null mean", sample => { sample.diff.meanAbsoluteChannelError = null; }],
  ["infinite mean", sample => { sample.diff.meanAbsoluteChannelError = Infinity; }],
  ["coerced ratio", sample => { sample.diff.differingRatio = "0"; }],
  ["negative error", sample => { sample.diff.meanAbsoluteChannelError = -1; }],
  ["inconsistent pixel ratio", sample => { sample.diff.differingPixels = 100; }],
  ["fractional pixel count", sample => { sample.diff.differingPixels = 0.5; sample.diff.differingRatio = 0.5 / (960 * 540); }],
  ["background change", sample => { sample.noon.background[0] = 1; }],
  ["missing background", sample => { delete sample.reference.background; }],
  ["null background", sample => { sample.noon.background[1] = null; }],
  ["blank reference", sample => { sample.reference.changedPixels = 0; }],
  ["blank candidate", sample => { sample.noon.changedPixels = 0; }],
  ["invalid image size", sample => { sample.noon.width = 961; }],
  ["missing bounds", sample => { sample.noon.bounds = null; }],
  ["stale bounds summary", sample => { sample.noon.bounds.minX += 5; sample.noon.bounds.maxX += 5; }],
  ["out-of-budget measured bounds", sample => { sample.noon.bounds.minX += 5; sample.noon.bounds.maxX += 5; sample.boundsDelta.centroidX = 5; }],
  ["missing bounds delta", sample => { delete sample.boundsDelta; }],
  ["out-of-image bounds", sample => { sample.noon.bounds.minX = -1; }],
  ["coerced sample time", sample => { sample.time = null; }],
]) test(`rejects falsely green ${name}`, () => {
  const { raster, semantic } = reports();
  mutate(raster.fixtures[0].backends.webgpu.samples[0]);
  assert.throws(() => assertFollowingReports(raster, semantic, manifest()));
});

test("accepts measured differences inside the unchanged policy", () => {
  const { raster, semantic } = reports();
  const sample = raster.fixtures[0].backends.webgpu.samples[0];
  sample.diff = { differingPixels: 1, differingRatio: 1 / (960 * 540), meanAbsoluteChannelError: 0.001 };
  assert.equal(assertFollowingReports(raster, semantic, manifest()).checks.length, FOLLOWING_TIMES.length * 2);
});

test("rejects report tolerance and reference configuration drift", () => {
  const { raster, semantic } = reports();
  raster.fixtures[0].backends.webgpu.tolerance.max_differing_ratio = 0.5;
  assert.throws(() => assertFollowingReports(raster, semantic, manifest()), /tolerance drift/);
  const other = reports();
  other.raster.reference.renderer = "opengl";
  assert.throws(() => assertFollowingReports(other.raster, other.semantic, manifest()), /configuration/);
  const subset = manifest();
  subset.fixtures[0].sample_times.pop();
  assert.throws(() => assertFollowingReports(other.raster, other.semantic, subset), /complete unmodified/);
});

test("camera failure preserves later regressions but still fails the job", () => {
  const workflow = readFileSync(new URL("../.github/workflows/manim-raster-differential.yml", import.meta.url), "utf8");
  const step = name => {
    const start = workflow.indexOf(`      - name: ${name}\n`);
    assert.ok(start >= 0, `missing workflow step ${name}`);
    const end = workflow.indexOf("\n      - name:", start + 1);
    return workflow.slice(start, end < 0 ? undefined : end);
  };
  const camera = step("Qualify pinned FollowingGraphCamera");
  assert.match(camera, /\n        id: following_graph_camera\n/);
  assert.match(camera, /\n        continue-on-error: true\n/);
  assert.match(camera, /run: node scripts\/special-camera-qualification\.mjs/);
  const canonical = step("Render and compare canonical Manim scenes");
  assert.doesNotMatch(canonical, /continue-on-error|following_graph_camera/);
  const final = step("Report deferred qualification failures");
  assert.match(final, /always\(\).*steps\.following_graph_camera\.outcome == 'failure' \|\| steps\.typst_ratchet\.outcome == 'failure'/);
  assert.match(final, /\n          exit 1\n/);
  assert.ok(workflow.indexOf(canonical) > workflow.indexOf(camera));
  assert.ok(workflow.indexOf(final) > workflow.indexOf(step("Verify shared sparse and dense playback at Manim frame times")));
});
