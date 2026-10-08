import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import vm from "node:vm";
import { FOLLOWING_SOURCE, FOLLOWING_TIMES, followingManifest,
  PINNED_CAMERA_FOLLOWUPS, assertSpecialCameraObservation, assertPairedCameraObservation,
  writeSpecialCameraObservations,
  assertFollowingSources, assertFollowingState, assertFollowingReports,
  assertFollowingPythonLifecycle, assertFollowingDirectLifecycle } from "./special-camera-qualification.mjs";
import { CAMERA_SOURCE_HASHES, PINNED_CAMERA_CASES, extractPinnedScene, PINNED_DOCS_COMMIT,
  PINNED_DOCS_SHA256 } from "./stage-special-camera-raster.mjs";

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

function cameraFixture(profile, sampleTime = 0) {
  if (profile === "moving-zoomed-scene" || profile === "following-graph-camera") {
    const file = profile === "moving-zoomed-scene" ? "moving_zoomed_scene_around" : "following_graph_camera";
    return { id: `${file}-${sampleTime}`, sampleTime, factory: "direct", file };
  }
  return { id: `${profile}-${sampleTime}`, sampleTime,
    factory: "createDirectSpecialCameraSettingsRenderer", factoryArgs: [profile] };
}

function semanticObservationFrame(fixture, hostTime = fixture.sampleTime) {
  const profile = fixture.factoryArgs?.[0] ?? (fixture.file === "moving_zoomed_scene_around"
    ? "moving-zoomed-scene" : "following-graph-camera");
  let spatial = { composition_domain: "world", material: "unlit", point_light: false,
    translation: [0, 0, 0] };
  if (profile === "fixed-frame") spatial.composition_domain = "fixed_frame";
  if (profile === "light" || profile === "surface") {
    spatial = { ...spatial, material: "point_lit", point_light: true,
      translation: profile === "light" ? [0, 0, -3] : [-7, -9, 10] };
  }
  const objects = [{ id: 1, present: true, center: [0, 0], bounds: { width: 2, height: 2 },
    spatial, fill: paint(1, 0, 0) }];
  if (profile === "light" || profile === "surface") {
    const cellSpatial = { composition_domain: "world", material: "point_lit",
      point_light: false, translation: [0, 0, 0] };
    objects.push({ id: 2, present: true, center: [1, 1], bounds: { width: 1, height: 1 },
      spatial: cellSpatial, fill: paint(1, 0, 0) });
    if (profile === "surface") objects.push({ id: 3, present: true, center: [2, 1],
      bounds: { width: 1, height: 1 }, spatial: structuredClone(cellSpatial), fill: paint(0, 1, 0) });
  }
  return { time: hostTime, camera: { center: [0, 0], height: 8 },
    present_object_count: objects.length, objects };
}

test("pinned followup inventory covers the six non-FollowingGraphCamera cases", () => {
  assert.deepEqual(PINNED_CAMERA_FOLLOWUPS, ["MovingZoomedSceneAround", "FixedInFrameMObjectTest",
    "ThreeDLightSourcePosition", "ThreeDCameraRotation", "ThreeDCameraIllusionRotation", "ThreeDSurfacePlot"]);
});

test("staged exact-source raster inputs pin all six upstream class hashes", async () => {
  assert.match(PINNED_DOCS_COMMIT, /^[a-f0-9]{40}$/);
  assert.match(PINNED_DOCS_SHA256, /^[a-f0-9]{64}$/);
  assert.deepEqual(Object.keys(CAMERA_SOURCE_HASHES), [
    "MovingZoomedSceneAround", "FixedInFrameMObjectTest", "ThreeDLightSourcePosition",
    "ThreeDCameraRotation", "ThreeDCameraIllusionRotation", "ThreeDSurfacePlot",
  ]);
  assert.ok(Object.values(CAMERA_SOURCE_HASHES).every(hash => /^[a-f0-9]{64}$/.test(hash)));
  const rst = [
    ".. manim:: RasterFixture",
    "    class RasterFixture(Scene):",
    "        def construct(self):",
    "            self.wait(1)",
  ].join("\n");
  const source = "class RasterFixture(Scene):\n    def construct(self):\n        self.wait(1)\n";
  const { createHash } = await import("node:crypto");
  const digest = createHash("sha256").update(source).digest("hex");
  assert.equal(extractPinnedScene(rst, "RasterFixture", digest), source);
  assert.throws(() => extractPinnedScene(rst, "RasterFixture", "0".repeat(64)), /source drift/);
});

test("pinned 3D camera samples cover motion handoffs and exact endpoints", () => {
  assert.deepEqual(PINNED_CAMERA_CASES.map(([scene]) => scene), Object.keys(CAMERA_SOURCE_HASHES));
  for (const [scene, duration, times, cameraObservation] of PINNED_CAMERA_CASES) {
    assert.equal(cameraObservation, scene !== "MovingZoomedSceneAround",
      `${scene}: 3D camera state must accompany raster observations`);
    assert.equal(times.at(-1), duration, `${scene}: missing exact source completion`);
    assert.ok(times.every((time, index) => Number.isFinite(time) && time >= 0
      && time <= duration && (index === 0 || time > times[index - 1])),
    `${scene}: samples must advance within authored time`);
  }
  const [, , rotationTimes] = PINNED_CAMERA_CASES.find(([scene]) => scene === "ThreeDCameraRotation");
  for (const barrier of [1, 2]) {
    for (const offset of [-1 / 30, 0, 1 / 30]) {
      assert.ok(rotationTimes.some(time => Math.abs(time - barrier - offset) < 1e-9),
        `camera motion handoff ${barrier}: missing observation at offset ${offset}`);
    }
  }
  assert.ok(rotationTimes.includes(1.5), "finite camera move needs an interior observation");
  const [, duration, illusionTimes] = PINNED_CAMERA_CASES.find(([scene]) => scene === "ThreeDCameraIllusionRotation");
  assert.ok(illusionTimes.includes(Math.floor(duration * 30) / 30),
    "retain the last materialized Manim frame separately from exact completion");
});

for (const profile of ["fixed-frame", "ambient", "illusion", "light", "surface",
  "moving-zoomed-scene", "following-graph-camera"]) {
  test(`camera observation records semantic content and authored time for ${profile}`, () => {
    const fixture = cameraFixture(profile, 0.5);
    const frame = semanticObservationFrame(fixture);
    const observation = assertSpecialCameraObservation(frame, fixture, "python");
    assert.equal(observation.authoredTime, 0.5);
    assert.equal(observation.pinnedAppearance, "not-qualified-by-this-noon-profile");
    assert.equal(assertPairedCameraObservation(frame, structuredClone(frame), fixture).semanticState,
      "paired-equal");
  });
}

for (const [name, profile, mutate] of [
  ["fixed-frame domain", "fixed-frame", frame => { frame.objects[0].spatial.composition_domain = "world"; }],
  ["ambient world content", "ambient", frame => { frame.objects[0].spatial.composition_domain = "fixed_frame"; }],
  ["illusion world content", "illusion", frame => { frame.objects[0].spatial.composition_domain = "fixed_frame"; }],
  ["point light position", "light", frame => { frame.objects[0].spatial.translation[2] = -2; }],
  ["point-lit surface", "surface", frame => {
    for (const row of frame.objects.filter(item => item.spatial.point_light !== true)) row.spatial.material = "unlit";
  }],
  ["effective camera", "moving-zoomed-scene", frame => { frame.camera.height = null; }],
  ["authored time", "ambient", frame => { frame.time += 0.1; }],
]) test(`camera semantic/timing observer rejects ${name}`, () => {
  const fixture = cameraFixture(profile, 0.5);
  const frame = semanticObservationFrame(fixture);
  mutate(frame);
  assert.throws(() => assertSpecialCameraObservation(frame, fixture, "python"));
});

test("paired camera observer rejects semantic drift while ignoring engine identity", () => {
  const fixture = cameraFixture("surface", 0);
  const rust = semanticObservationFrame(fixture);
  const python = structuredClone(rust);
  python.objects[0].id = 1001;
  assert.equal(assertPairedCameraObservation(rust, python, fixture).semanticState, "paired-equal");
  python.objects.reverse();
  assert.throws(() => assertPairedCameraObservation(rust, python, fixture), /differs from/);
  python.objects.reverse();
  python.objects[0].fill.alpha = 0.5;
  assert.throws(() => assertPairedCameraObservation(rust, python, fixture), /differs from/);
});

test("paired camera observer uses the inclusive raw 1e-6 semantic tolerance", () => {
  const fixture = cameraFixture("fixed-frame", 0.5);
  const rust = semanticObservationFrame(fixture);
  const python = structuredClone(rust);
  python.objects[0].center[0] += 0.999e-6;
  const observation = assertPairedCameraObservation(rust, python, fixture);
  assert.ok(observation.maximumAbsoluteError <= 1e-6);
  python.objects[0].center[0] = rust.objects[0].center[0] + 1.001e-6;
  assert.throws(() => assertPairedCameraObservation(rust, python, fixture), /differs from/);
});

test("camera observation artifact records selected coverage and appearance limitation", async () => {
  const previousSelection = process.env.NOON_PAIRED_CASES;
  delete process.env.NOON_PAIRED_CASES;
  const directory = mkdtempSync(path.join(os.tmpdir(), "noon-camera-observation-"));
  try {
    const fixture = cameraFixture("fixed-frame", 0.5);
    const frame = semanticObservationFrame(fixture);
    const stem = path.join(directory, `${fixture.id}-WebGPU`);
    writeFileSync(`${stem}-rust-wasm-frame.json`, JSON.stringify(frame));
    writeFileSync(`${stem}-python-frame.json`, JSON.stringify(frame));
    const observation = await writeSpecialCameraObservations({ backends: [
      { backend: "WebGPU", static: { [fixture.id]: {} } },
    ] }, [fixture], directory);
    assert.equal(observation.passed, true);
    assert.equal(observation.coverageComplete, false);
    assert.deepEqual(observation.selectedFixtureIds, [fixture.id]);
    assert.equal(observation.appearanceQualification, "deferred-to-pinned-Manim-raster-oracle");
    const artifact = JSON.parse(readFileSync(path.join(directory, "semantic-observations.json"), "utf8"));
    assert.deepEqual(artifact, observation);
  } finally {
    if (previousSelection === undefined) delete process.env.NOON_PAIRED_CASES;
    else process.env.NOON_PAIRED_CASES = previousSelection;
    rmSync(directory, { recursive: true, force: true });
  }
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

function pythonLifecycle() {
  const oracle = reports().semantic.fixtures[0];
  oracle.frames[0].camera = { center: [0, 0], height: 8 };
  oracle.frames[45].camera = { center: [0, -0.5], height: 4 };
  oracle.terminal_state.camera = { center: [0, 0], height: 8 };
  const makeRun = () => {
    const frame = noonFrame(oracle.terminal_state);
    frame.publication = { scene_revision: 4, execution_revision: 8, frame_epoch: 12 };
    const before = { state: { time: 3, playing: false, replaySupported: false,
      replayUnavailable: "UnsupportedDomain" }, frame };
    return { duration: 3, backend: "WebGPU", presentedFrames: 1,
      samples: [noonFrame(oracle.frames[0]), noonFrame(oracle.frames[45]), noonFrame(oracle.terminal_state)],
      before, controls: ["seek", "restartPlayback", "resume"].map(operation => ({ operation,
        denial: "Error: Replay unavailable: UnsupportedDomain", after: structuredClone(before) })) };
  };
  return { observation: { runs: [makeRun(), makeRun()] }, oracle };
}

test("Python lifecycle requires completed-source denial and a fresh Run", () => {
  const { observation, oracle } = pythonLifecycle();
  assert.deepEqual(assertFollowingPythonLifecycle(observation, oracle, "WebGPU"),
    { completedRuns: 2, replay: "denied", denialKind: "unsupported-domain", failureAtomic: true, freshRun: true });
});

for (const [name, mutate] of [
  ["execution failure", data => { data.error = "worker crashed"; }],
  ["no fresh Run", data => { data.runs.pop(); }],
  ["unfinished source", data => { data.runs[0].duration = null; }],
  ["wrong backend", data => { data.runs[0].backend = "WebGL2"; }],
  ["no presentation", data => { data.runs[0].presentedFrames = 0; }],
  ["missing follow sample", data => { data.runs[0].samples.splice(1, 1); }],
  ["stale fresh-run camera", data => { data.runs[1].samples[0].camera.height = 4; }],
  ["changed following state", data => { data.runs[0].samples[1].camera.center[0] += 0.01; }],
  ["missing replay admission", data => { delete data.runs[0].before.state.replaySupported; }],
  ["replay falsely allowed", data => { data.runs[0].before.state.replaySupported = true; }],
  ["unrelated denial reason", data => { data.runs[0].before.state.replayUnavailable = "source still running"; }],
  ["missing domain classification", data => { data.runs[0].before.state.replayUnavailable = null; }],
  ["incomplete replay", data => { data.runs[0].before.state.replayUnavailable = "Incomplete"; }],
  ["exhausted replay budget", data => { data.runs[0].before.state.replayUnavailable = "RetentionLimit"; }],
  ["unrecorded input", data => { data.runs[0].before.state.replayUnavailable = "UnrecordedInput"; }],
  ["invented callback prose", data => { data.runs[0].before.state.replayUnavailable = "opaque host callbacks cannot be replayed"; }],
  ["different replay-control failure", data => { data.runs[0].controls[0].denial = "Error: Replay unavailable: RetentionLimit"; }],
  ["unwrapped domain error", data => { data.runs[0].controls[1].denial = "UnsupportedDomain"; }],
  ["unrelated error containing replay text", data => { data.runs[0].controls[2].denial = "TypeError: Replay unavailable: UnsupportedDomain"; }],
  ["clock still playing", data => { data.runs[0].before.state.playing = true; }],
  ["handoff changed time", data => { data.runs[0].before.state.time = 0; }],
  ["missing publication identity", data => { delete data.runs[0].before.frame.publication; }],
  ["handoff changed camera", data => { data.runs[0].before.frame.camera.height = 4; }],
  ["missing loop control", data => { data.runs[0].controls.splice(1, 1); }],
  ["accepted rewind", data => { data.runs[0].controls[0].denial = null; }],
  ["busy continuation instead of replay denial", data => { data.runs[0].controls[0].denial = "playback controls are unavailable while a Python source continuation owns execution"; }],
  ["failed control changed effective state", data => { data.runs[0].controls[0].after.frame.camera.height = 4; }],
  ["failed control changed revision", data => { data.runs[0].controls[0].after.frame.publication.frame_epoch++; }],
  ["failed control changed playback", data => { data.runs[0].controls[2].after.state.playing = true; }],
]) test(`Python lifecycle rejects ${name}`, () => {
  const { observation, oracle } = pythonLifecycle();
  mutate(observation);
  assert.throws(() => assertFollowingPythonLifecycle(observation, oracle, "WebGPU"));
});


function directLifecycle() {
  const { observation: python, oracle } = pythonLifecycle();
  const before = structuredClone(python.runs[0].before.frame);
  return { oracle, observation: { before, after: structuredClone(before),
    backend: "WebGPU", cadence: "idle",
    denial: "typed execution APIs require a direct session source" } };
}

test("direct continuation ownership denial is distinct from Python callback replay admission", () => {
  const { observation, oracle } = directLifecycle();
  assert.deepEqual(assertFollowingDirectLifecycle(observation, oracle, "WebGPU"), {
    replay: "denied", denialKind: "live-program-ownership", failureAtomic: true,
    reason: observation.denial,
  });
  // The old assertion rejected the actual source-ownership guard, despite the
  // denied seek preserving state. Do not replace it with an accept-any-error test.
  assert.doesNotMatch(observation.denial, /replay|callback|continuation/i);
});

for (const [name, mutate] of [
  ["source failure", data => { data.error = "source failed"; }],
  ["wrong backend", data => { data.backend = "WebGL2"; }],
  ["unfinished continuation", data => { data.cadence = "animation-frame"; }],
  ["missing publication", data => { delete data.before.publication; }],
  ["wrong endpoint", data => { data.before.time = 2; }],
  ["accepted seek", data => { data.denial = null; }],
  ["presentation barrier", data => { data.denial = "direct execution host must present pending runtime changes before advancing again"; }],
  ["missing API", data => { data.denial = "TypeError: renderer.seekDirect is not a function"; }],
  ["callback denial on wrong source type", data => { data.denial = "opaque host callback sessions do not support seek or replay"; }],
  ["changed camera", data => { data.after.camera.height = 4; }],
  ["changed publication", data => { data.after.publication.frame_epoch++; }],
]) test(`direct lifecycle rejects ${name}`, () => {
  const { observation, oracle } = directLifecycle();
  mutate(observation);
  assert.throws(() => assertFollowingDirectLifecycle(observation, oracle, "WebGPU"));
});


// Execute the harness's actual browser handoff, not a copied implementation.
// The interpreter/renderer stay mocked here; real two-backend lifecycle runs
// remain required and their replay/state assertions are deliberately unchanged.
const harness = readFileSync(new URL("./special-camera-qualification.mjs", import.meta.url), "utf8");
const handoffStart = harness.indexOf("                  const result = await completed;");
const handoffEnd = harness.indexOf("                  let metrics;", handoffStart);
assert.ok(handoffStart >= 0 && handoffEnd > handoffStart, "camera lifecycle handoff must exist");
const completeCameraSource = vm.runInNewContext(
  `(async ({ completed, registered, run, execution, authoring }) => {
    ${harness.slice(handoffStart, handoffEnd)}
  })`,
);

function sourceHandoff() {
  const registered = { contextId: 17, callbackSessionId: 31, continuationGeneration: 43 };
  const result = { duration: 3, semanticExecution: { ...registered } };
  const calls = [];
  const authoring = {};
  const execution = {
    async reconcileSemanticExecution(descriptor, options) {
      // Reattaching a completed non-null generation reproduces the worker's
      // stale-attachment guard; it is not a successful callback replay denial.
      assert.equal(descriptor.continuationGeneration, null, "stale semantic continuation attachment");
      assert.equal(options.authoringClient, authoring);
      calls.push({ descriptor: { ...descriptor }, loopDurationSeconds: options.loopDurationSeconds });
    },
    async advanceTo() { assert.fail("handoff must preserve the endpoint without repairing authored time"); },
  };
  return { registered, result, calls, authoring, execution, run: {} };
}

test("Python source handoff waits for completion and retires only the continuation token", async () => {
  const state = sourceHandoff();
  const original = structuredClone(state.result);
  let release;
  const completed = new Promise(resolve => { release = resolve; });
  const pending = completeCameraSource({ ...state, completed });
  await Promise.resolve();
  assert.deepEqual(state.calls, []);
  assert.deepEqual(state.run, {});
  release(state.result);
  await pending;
  assert.equal(state.run.duration, 3);
  assert.deepEqual(state.calls, [{ descriptor: {
    contextId: 17, callbackSessionId: 31, continuationGeneration: null,
  }, loopDurationSeconds: 4 }]);
  assert.deepEqual(state.result, original, "the source result retains its original identity evidence");
  assert.equal(state.registered.continuationGeneration, 43);
});

for (const [name, mutate] of [
  ["missing completed descriptor", state => { delete state.result.semanticExecution; }],
  ["different context", state => { state.result.semanticExecution.contextId++; }],
  ["different generation", state => { state.result.semanticExecution.continuationGeneration++; }],
  ["prematurely cleared generation", state => { state.result.semanticExecution.continuationGeneration = null; }],
  ["missing registered source", state => { state.registered = null; }],
]) test(`Python source handoff rejects ${name} before reattachment`, async () => {
  const state = sourceHandoff();
  mutate(state);
  await assert.rejects(completeCameraSource({ ...state, completed: Promise.resolve(state.result) }),
    /completed semantic source replaced its first-play continuation/);
  assert.deepEqual(state.calls, []);
});

test("Python source handoff preserves completion errors without reattachment", async () => {
  const state = sourceHandoff();
  const failure = new Error("source completion failed");
  await assert.rejects(completeCameraSource({ ...state, completed: Promise.reject(failure) }),
    error => error === failure);
  assert.deepEqual(state.calls, []);
  assert.deepEqual(state.run, {});
});

test("Python source handoff propagates reattachment failure without advancing the scene", async () => {
  const state = sourceHandoff();
  const failure = new Error("renderer handoff failed");
  state.execution.reconcileSemanticExecution = async () => { throw failure; };
  await assert.rejects(completeCameraSource({ ...state, completed: Promise.resolve(state.result) }),
    error => error === failure);
});
