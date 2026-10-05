// Capability pairs and the pinned FollowingGraphCamera oracle share the existing
// authoring/raster runners. Oracle mode adds evidence, never scene semantics.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { evaluateRasterTolerance, resolveRasterTolerance } from "./manim-raster-policy.mjs";

export const FOLLOWING_SOURCE = "parity/manim-v0.21/following_graph_camera.py";
export const FOLLOWING_UPSTREAM = "861cd4849b17db1db3515b531ffe80b297848f93";
// Actual 30 Hz reference frames, including both sides of the two barriers.
export const FOLLOWING_TIMES = Object.freeze([
  0, 0.5, 29 / 30, 1, 31 / 30, 1.2, 1.5, 1.8, 59 / 30,
  2, 61 / 30, 2.5, 89 / 30,
]);
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export function followingManifest(baseline) {
  assert.equal(baseline.reference.version, "0.21.0");
  assert.equal(baseline.reference.renderer, "cairo");
  assert.equal(baseline.reference.frame_rate, 30);
  assert.deepEqual([baseline.reference.pixel_width, baseline.reference.pixel_height], [960, 540]);
  return {
    ...baseline,
    reference: { ...baseline.reference, source: FOLLOWING_SOURCE },
    fixtures: [{ id: "following-graph-camera", scene: "FollowingGraphCamera",
      source: FOLLOWING_SOURCE, expected_duration: 3, sample_times: [...FOLLOWING_TIMES] }],
  };
}

// Compare the complete class AST, not a text substring or just the final frame.
// This is source-only validation; it neither imports Manim nor executes a scene.
export function assertFollowingSources(reference, noon, upstream) {
  const result = spawnSync("python3", ["-c", String.raw`
import ast, json, re, sys, textwrap
reference, noon, upstream = json.load(sys.stdin)
lines = upstream.splitlines()
starts = [i for i, line in enumerate(lines) if re.match(r"^\s*class FollowingGraphCamera\(", line)]
assert len(starts) == 1, "expected exactly one pinned upstream camera class"
start = starts[0]
indent = len(lines[start]) - len(lines[start].lstrip())
end = start + 1
while end < len(lines) and (not lines[end].strip() or len(lines[end]) - len(lines[end].lstrip()) >= indent):
    end += 1
upstream_class = textwrap.dedent("\n".join(lines[start:end]))
def camera(source):
    module = ast.parse(source)
    classes = [node for node in module.body if isinstance(node, ast.ClassDef)]
    assert len(classes) == 1 and classes[0].name == "FollowingGraphCamera", "unexpected source class"
    for node in module.body:
        assert isinstance(node, (ast.Import, ast.ImportFrom, ast.ClassDef)) or (
            isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant)
            and isinstance(node.value.value, str)), "executable top-level source adaptation"
    return ast.dump(classes[0], include_attributes=False)
expected = camera(upstream_class)
assert camera(reference) == expected, "canonical FollowingGraphCamera differs from pinned upstream"
assert camera(noon) == expected, "Noon FollowingGraphCamera differs from pinned upstream"
`], { input: JSON.stringify([reference, noon, upstream]), encoding: "utf8", timeout: 10_000 });
  assert.equal(result.status, 0, result.stderr || String(result.error));
  return { upstreamCommit: FOLLOWING_UPSTREAM, upstreamSha256: hash(upstream),
    referenceSha256: hash(reference), noonSha256: hash(noon) };
}

export function assertFollowingState(actual, expected, label = "FollowingGraphCamera") {
  let maximumAbsoluteError = 0;
  const near = (left, right, field) => {
    assert.equal(typeof left, "number", `${label}: missing/non-numeric ${field}`);
    assert.equal(typeof right, "number", `${label}: invalid oracle ${field}`);
    assert.ok(Number.isFinite(left) && Number.isFinite(right), `${label}: non-finite ${field}`);
    const error = Math.abs(left - right);
    assert.ok(error <= 1e-6, `${label}: ${field}: ${left} != ${right}`);
    maximumAbsoluteError = Math.max(maximumAbsoluteError, error);
  };
  near(actual.time, expected.time, "time");
  assert.ok(actual.camera && expected.camera, `${label}: missing effective camera`);
  for (let axis = 0; axis < 2; axis++) near(actual.camera.center?.[axis], expected.camera.center?.[axis], `camera.center[${axis}]`);
  near(actual.camera.height, expected.camera.height, "camera.height");
  assert.ok(Array.isArray(actual.objects) && Array.isArray(expected.objects), `${label}: missing objects`);
  const referenceDots = expected.objects.filter(row => row.type === "Dot");
  assert.equal(referenceDots.length, 3, `${label}: all three pinned dots must remain present`);
  // The default 0.16-diameter dots are the only filled objects of this size.
  // Match independent engine identities by observable geometry/paint, not IDs.
  const dots = actual.objects.filter(row => row.present && row.fill?.alpha > 0
    && Math.abs(row.bounds?.width - 0.16) < 1e-4 && Math.abs(row.bounds?.height - 0.16) < 1e-4);
  assert.equal(dots.length, 3, `${label}: all three retained dots must remain present`);
  const remaining = [...dots];
  for (const [index, expectedDot] of referenceDots.entries()) {
    const at = remaining.findIndex(row => ["red", "green", "blue", "alpha"].every(channel =>
      typeof row.fill?.[channel] === "number" && Number.isFinite(row.fill[channel])
      && Math.abs(row.fill[channel] - expectedDot.fill[channel]) <= 1e-6)
      && row.center?.every((value, axis) => typeof value === "number"
        && Math.abs(value - expectedDot.center[axis]) <= 1e-6));
    assert.notEqual(at, -1, `${label}: dot ${index} effective center/color differs from Manim`);
    const [dot] = remaining.splice(at, 1);
    for (let axis = 0; axis < 2; axis++) near(dot.center[axis], expectedDot.center[axis], `dot[${index}].center[${axis}]`);
    for (const dimension of ["width", "height"]) near(dot.bounds[dimension], expectedDot.bounds[dimension], `dot[${index}].${dimension}`);
    for (const channel of ["red", "green", "blue", "alpha"]) near(dot.fill[channel], expectedDot.fill[channel], `dot[${index}].${channel}`);
  }
  return { maximumAbsoluteError, dotCount: dots.length };
}

function assertFollowingRasterSample(sample, reference, tolerance, label) {
  const pixels = reference.pixel_width * reference.pixel_height;
  const finite = (value, name, min, max) => {
    assert.ok(typeof value === "number" && Number.isFinite(value) && value >= min && value <= max,
      `${label}: invalid ${name}`);
  };
  for (const host of ["reference", "noon"]) {
    const image = sample[host];
    assert.equal(image?.width, reference.pixel_width, `${label}: ${host} width`);
    assert.equal(image?.height, reference.pixel_height, `${label}: ${host} height`);
    assert.ok(Number.isInteger(image.changedPixels) && image.changedPixels > 0 && image.changedPixels <= pixels,
      `${label}: ${host} must contain measured foreground`);
    assert.ok(Array.isArray(image.background) && image.background.length === 4,
      `${label}: missing ${host} background`);
    for (const channel of image.background) {
      finite(channel, `${host} background channel`, 0, 255);
      assert.ok(Number.isInteger(channel), `${label}: background channel must be a byte`);
    }
    const bounds = image.bounds;
    assert.ok(bounds, `${label}: missing ${host} foreground bounds`);
    for (const [low, high, extent] of [["minX", "maxX", image.width], ["minY", "maxY", image.height]]) {
      finite(bounds[low], `${host} ${low}`, 0, extent - 1);
      finite(bounds[high], `${host} ${high}`, bounds[low], extent - 1);
      assert.ok(Number.isInteger(bounds[low]) && Number.isInteger(bounds[high]),
        `${label}: foreground bounds must be pixel indices`);
    }
  }
  const expected = sample.reference.bounds;
  const actual = sample.noon.bounds;
  // Re-derive bounds deltas from measured endpoints: a stale zero summary
  // must not conceal a displaced or incorrectly sized image.
  const boundsDelta = {
    centroidX: (actual.minX + actual.maxX - expected.minX - expected.maxX) / 2,
    centroidY: (actual.minY + actual.maxY - expected.minY - expected.maxY) / 2,
    width: actual.maxX - actual.minX - (expected.maxX - expected.minX),
    height: actual.maxY - actual.minY - (expected.maxY - expected.minY),
  };
  assert.deepEqual(sample.boundsDelta, boundsDelta, `${label}: inconsistent bounds deltas`);
  assert.ok(sample.diff, `${label}: missing raw raster metrics`);
  finite(sample.diff.differingRatio, "differingRatio", 0, 1);
  finite(sample.diff.meanAbsoluteChannelError, "meanAbsoluteChannelError", 0, 255);
  finite(sample.diff.differingPixels, "differingPixels", 0, pixels);
  assert.ok(Number.isInteger(sample.diff.differingPixels), `${label}: non-integer pixel count`);
  assert.ok(Math.abs(sample.diff.differingRatio - sample.diff.differingPixels / pixels) <= 1e-12,
    `${label}: inconsistent differing pixel ratio`);
  const policy = evaluateRasterTolerance({ sample: { ...sample, boundsDelta }, timingDelta: 0, tolerance });
  assert.equal(policy.passed, true, `${label}: raw raster policy failed: ${JSON.stringify(policy.failures)}`);
  assert.deepEqual(sample.categories, policy.categories, `${label}: inconsistent raster classification`);
}

export function assertFollowingReports(raster, semantic, manifest) {
  const focused = followingManifest(manifest);
  assert.deepEqual(manifest.fixtures, focused.fixtures, "camera manifest must retain the complete unmodified fixture");
  assert.deepEqual(raster.reference, manifest.reference, "raster source/configuration must match the requested oracle");
  const tolerance = resolveRasterTolerance(manifest, manifest.fixtures[0]);
  assert.equal(semantic.manim_version, "0.21.0");
  assert.equal(semantic.frame_rate, 30);
  assert.equal(semantic.fixtures?.length, 1);
  const oracle = semantic.fixtures[0];
  assert.equal(oracle.id, "following-graph-camera");
  assert.equal(oracle.logical_duration, 3);
  assert.equal(oracle.frame_count, 90);
  assert.equal(oracle.frames.length, 90);
  oracle.frames.forEach((frame, index) => {
    assert.equal(frame.frame_index, index);
    assert.equal(typeof frame.time, "number");
    assert.ok(Number.isFinite(frame.time));
    assert.ok(Math.abs(frame.time - index / 30) < 1e-9, `reference frame clock ${index}`);
  });
  assert.equal(oracle.terminal_state?.time, 3, "reference must capture logical completion separately from the final PNG");
  assert.equal(raster.enforce, true, "report-only raster measurements cannot qualify the scene");
  assert.equal(raster.fixtures?.length, 1);
  const fixture = raster.fixtures[0];
  assert.equal(fixture.id, oracle.id);
  assert.deepEqual(Object.keys(fixture.backends).sort(), ["webgl", "webgpu"]);
  const checks = [];
  for (const [backend, entry] of Object.entries(fixture.backends)) {
    assert.equal(entry.error, undefined, `${backend}: execution failure`);
    assert.deepEqual(entry.tolerance, tolerance, `${backend}: raster tolerance drift`);
    assert.equal(entry.noonDuration, 3, `${backend}: exact source completion`);
    assert.equal(entry.durationDelta, 0, `${backend}: exact logical duration`);
    assert.equal(entry.samples.length, FOLLOWING_TIMES.length, `${backend}: incomplete sample set`);
    entry.samples.forEach((sample, index) => {
      assert.ok(typeof sample.time === "number" && Number.isFinite(sample.time), `${backend}: invalid sample clock`);
      assert.ok(Math.abs(sample.time - FOLLOWING_TIMES[index]) < 1e-9, `${backend}: sample clock/order`);
      assert.equal(sample.frameIndex, Math.round(FOLLOWING_TIMES[index] * 30));
      assertFollowingRasterSample(sample, manifest.reference, tolerance, `${backend}@${sample.time}`);
      checks.push({ backend, time: sample.time, ...assertFollowingState(sample.debugFrame,
        oracle.frames[sample.frameIndex], `${backend}@${sample.time}`) });
    });
  }
  return { oracle, checks };
}

async function followingOracle(qualifyPairedAuthoring) {
  assert.equal(process.env.NOON_PAIRED_CASES, undefined, "full oracle qualification cannot select a passing subset");
  const output = path.resolve(root, process.env.NOON_CAMERA_ARTIFACTS ?? "browser-smoke-artifacts/following-graph-camera");
  await rm(output, { recursive: true, force: true });
  await mkdir(output, { recursive: true });
  const result = { passed: false, scope: "pinned FollowingGraphCamera; not physical-device performance" };
  try {
    const reference = await readFile(path.join(root, FOLLOWING_SOURCE), "utf8");
    const noon = await readFile(path.join(root, "web/python/examples/manim_example_following_graph_camera.py"), "utf8");
    const url = `https://raw.githubusercontent.com/ManimCommunity/manim/${FOLLOWING_UPSTREAM}/docs/source/examples.rst`;
    const response = await fetch(url, { signal: AbortSignal.timeout(30_000) });
    assert.ok(response.ok, `pinned upstream fetch failed: ${response.status}`);
    const upstream = await response.text();
    assert.ok(upstream.length < 256_000, "unexpected upstream source size");
    await writeFile(path.join(output, "upstream-examples.rst"), upstream);
    result.sources = assertFollowingSources(reference, noon, upstream);
    result.revision = spawnSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).stdout.trim();
    assert.match(result.revision, /^[a-f0-9]{40}$/);
    result.packages = await Promise.all(["noon_web.js", "noon_web_bg.wasm"].map(async file => {
      const bytes = await readFile(path.join(root, "web/pkg", file));
      return { file, bytes: bytes.length, sha256: hash(bytes) };
    }));
    const baseline = JSON.parse(await readFile(path.join(root, "parity/manim-v0.21/manifest.json"), "utf8"));
    const manifest = followingManifest(baseline);
    const manifestPath = path.join(output, "manifest.json");
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    const rasterOutput = path.join(output, "raster");
    const run = spawnSync(process.execPath, ["scripts/manim-raster-differential.mjs"], {
      cwd: root, encoding: "utf8", timeout: 600_000, maxBuffer: 16 * 1024 * 1024,
      env: { ...process.env, NOON_MANIM_RASTER_MANIFEST: manifestPath,
        NOON_MANIM_RASTER_ARTIFACTS: rasterOutput, NOON_MANIM_RASTER_ENFORCE: "1",
        NOON_MANIM_RASTER_BACKENDS: "webgpu,webgl" },
    });
    await writeFile(path.join(output, "raster-command.log"), `${run.stdout ?? ""}\n${run.stderr ?? ""}\n${run.error ?? ""}`);
    console.log((run.stdout ?? "").split("\n").slice(-12).join("\n"));
    result.rasterExitStatus = run.status;
    result.rasterSignal = run.signal;
    result.rasterProcessError = run.error ? String(run.error) : null;
    assert.equal(run.status, 0, `pinned raster failed; see ${output}/raster-command.log\n${run.stderr ?? run.error ?? ""}`);
    const raster = JSON.parse(await readFile(path.join(rasterOutput, "report.json"), "utf8"));
    const semantic = JSON.parse(await readFile(path.join(rasterOutput, "semantic/manim-all-frames.json"), "utf8"));
    // Raw raster failures remain in the child report and command log.
    console.log(JSON.stringify(raster.fixtures.map(fixture => ({ id: fixture.id,
      backends: Object.fromEntries(Object.entries(fixture.backends).map(([backend, entry]) => [backend,
        entry.error ? { error: entry.error } : entry.samples.map(sample => ({
          time: sample.time, diff: sample.diff, camera: sample.debugFrame.camera,
          oracleCamera: semantic.fixtures[0].frames[sample.frameIndex].camera,
        }))])) })), null, 2));
    const { oracle, checks } = assertFollowingReports(raster, semantic, manifest);
    result.semantic = checks;
    const { PNG } = await import("pngjs");
    const pairedOutput = path.join(output, "paired");
    const cases = [...FOLLOWING_TIMES, 3].map(sampleTime => ({
      id: `following_graph_camera-${sampleTime}`, sourcePath: FOLLOWING_SOURCE,
      scene: "FollowingGraphCamera", factory: "createDirectFollowingGraphCameraRenderer",
      duration: 3, sampleTime, playback: "live", boundaries: [1, 2],
    }));
    result.paired = await qualifyPairedAuthoring({ cases, artifactDirectory: pairedOutput,
      qualifyLifecycle: async (context, baseUrl) => {
        const page = await context.newPage();
        try {
          await page.goto(`${baseUrl}/web/manim-raster-host.html`);
          const observation = await page.evaluate(async () => {
            const wasm = await import("./pkg/noon_web.js");
            await wasm.default();
            const canvas = document.querySelector("#scene");
            const renderer = await wasm.createDirectFollowingGraphCameraRenderer(canvas.transferControlToOffscreen());
            const { sampleDirectProgram } = await import("../scripts/direct-program-sample.mjs");
            renderer.advanceDirectRealtime(0);
            for (const time of [0, 1, 2, 3]) await sampleDirectProgram(renderer, time);
            const before = JSON.parse(renderer.debugSelectionFrameJson());
            if (typeof renderer.seekDirect !== "function") throw new Error("missing direct replay API");
            let denial = null;
            try { renderer.seekDirect(1.5); } catch (error) { denial = String(error); }
            const after = JSON.parse(renderer.debugSelectionFrameJson());
            return { before, after, denial };
          });
          assert.equal(observation.before.time, 3);
          assert.match(observation.denial ?? "", /replay|callback|continuation/i,
            "opaque callbacks must reject rewind through the shared engine");
          assert.deepEqual(observation.after, observation.before, "rejected rewind must preserve the complete effective publication");
          return { replay: "denied", failureAtomic: true, reason: observation.denial };
        } finally { await page.close(); }
      },
      qualifyPixels: async ({ fixture, backend, rust, python }) => {
        const expected = fixture.sampleTime === 3 ? oracle.terminal_state
          : oracle.frames[Math.round(fixture.sampleTime * 30)];
        const semanticChecks = [];
        for (const host of ["rust-wasm", "python"]) {
          const frame = JSON.parse(await readFile(path.join(pairedOutput, `${fixture.id}-${backend}-${host}-frame.json`), "utf8"));
          semanticChecks.push({ host, ...assertFollowingState(frame, expected, `${fixture.id}/${backend}/${host}`) });
        }
        if (fixture.sampleTime < 3) {
          const label = `frame-${String(expected.frame_index).padStart(4, "0")}.png`;
          const dense = PNG.sync.read(await readFile(path.join(rasterOutput,
            backend === "WebGPU" ? "webgpu" : "webgl", "following-graph-camera", label)));
          assert.equal(dense.width, rust.width);
          assert.equal(dense.height, rust.height);
          assert.deepEqual(rust.data, dense.data, "sparse Rust frame differs from the oracle-qualified dense Python frame");
          assert.deepEqual(python.data, dense.data, "sparse Python frame differs from dense forward execution");
        }
        return { semanticChecks, denseForwardCompared: fixture.sampleTime < 3 };
      },
    });
    result.passed = true;
    console.log(`[PASS] FollowingGraphCamera: ${checks.length} pinned raster/state samples; paired sparse/dense hosts and exact logical endpoint`);
  } catch (error) {
    result.error = error.stack ?? String(error);
    throw error;
  } finally {
    await writeFile(path.join(output, "qualification.json"), `${JSON.stringify(result, null, 2)}\n`);
  }
}

async function main() {
  const { qualifyPairedAuthoring } = await import("./paired-authoring-qualification.mjs");
  if (process.env.NOON_CAMERA_MANIM_ORACLE === "1") return followingOracle(qualifyPairedAuthoring);
  const { disableAuthoringJspi } = await import("./playground-browser-support.mjs");
  const { default: playwright } = await import("playwright");
  // Other camera cases remain explicit Noon-profile pairs, not Cairo claims.
  const spatial = [
    ["fixed_in_frame_mobject_test", "fixed-frame", 1, [1]],
    ["three_d_camera_rotation", "ambient", 3, [0.5, 2.5, 3]],
    ["three_d_camera_illusion_rotation", "illusion", Math.PI / 2, [0.5, Math.PI / 2]],
    ["three_d_light_source_position", "light", 0, [0]],
    ["three_d_surface_plot", "surface", 0, [0]],
  ];
  const cases = spatial.flatMap(([name, profile, duration, samples]) => samples.map(sampleTime => ({
    id: `${profile}-${sampleTime}`, file: `manim_example_${name}.py`,
    factory: "createDirectSpecialCameraSettingsRenderer", factoryArgs: [profile],
    duration, sampleTime, playback: duration > 0 ? "live" : undefined, boundaries: [1, 2],
    directHeldSampleTime: profile === "ambient" && sampleTime === 2.5 ? 2 : undefined,
  })));
  for (const [name, factory, duration, samples] of [
    ["following_graph_camera", "createDirectFollowingGraphCameraRenderer", 3, [0.5, 1.5, 2.5, 3]],
    ["moving_zoomed_scene_around", "createDirectMovingZoomedSceneAroundRenderer", 12, [0.5, 1.5, 3.5, 9.5, 12]],
  ]) {
    for (const sampleTime of samples) cases.push({
      id: `${name}-${sampleTime}`, file: `manim_example_${name}.py`,
      factory, duration, sampleTime, playback: "live",
      boundaries: Array.from({ length: duration }, (_, index) => index + 1),
    });
  }
  const browserName = process.env.NOON_CAMERA_BROWSER ?? "chromium";
  const noJspi = process.env.NOON_CAMERA_NO_JSPI === "1" || browserName === "webkit";
  const contextOptions = browserName === "webkit" ? { ...playwright.devices["iPhone 13"] } : undefined;
  const output = process.env.NOON_CAMERA_ARTIFACTS ?? "browser-smoke-artifacts/special-camera";
  await qualifyPairedAuthoring({ cases, prepareContext: noJspi ? disableAuthoringJspi : undefined,
    browserName, contextOptions, artifactDirectory: noJspi ? `${output}/no-jspi` : output });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
