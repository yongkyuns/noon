import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { serveRepository } from "./browser-test-server.mjs";
import { mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";
import {
  browserArgs,
  dominantImageRgba,
  rasterFixtureSource,
  sampleRasterFrames,
} from "./manim-raster-support.mjs";
import { evaluateRasterTolerance, formatRasterPolicyFailure, resolveRasterTolerance } from "./manim-raster-policy.mjs";
import { compareForegroundCoverage } from "./browser-visual-parity-lib.mjs";

const { chromium } = playwright;
const { PNG } = pngjs;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const manifestPath = path.resolve(repoRoot, process.env.NOON_MANIM_RASTER_MANIFEST ?? "parity/manim-v0.21/manifest.json");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const reference = manifest.reference;
const referenceFontPaths = JSON.parse(process.env.NOON_MANIM_REFERENCE_FONTS ?? "[]");
assert.ok(Array.isArray(referenceFontPaths) && referenceFontPaths.every(value => typeof value === "string"),
  "NOON_MANIM_REFERENCE_FONTS must be a JSON array of font paths");
// These are the explicit family used by the Text fixtures and Pango's generic
// family selected by the MarkupText fixture's <tt> run.
const referenceFontFamilies = ["DejaVu Sans Mono", "monospace"];
const referenceFonts = await Promise.all([
  ...referenceFontFamilies.map(async requestedFamily => {
    const result = spawnSync("fc-match", [
      "--format", "%{family[0]}\n%{style[0]}\n%{file}\n%{index}\n", requestedFamily,
    ], { encoding: "utf8" });
    assert.equal(result.status, 0, `fc-match failed for ${requestedFamily}: ${result.stderr ?? ""}`);
    const [resolvedFamily, style, font, index] = result.stdout.trimEnd().split("\n");
    assert.ok(resolvedFamily && style && font && index !== undefined,
      `fc-match returned incomplete metadata for ${requestedFamily}`);
    return {
      requestedFamily,
      resolvedFamily,
      style,
      path: path.resolve(font),
      index: Number(index),
      sha256: createHash("sha256").update(await readFile(font)).digest("hex"),
    };
  }),
  ...referenceFontPaths.map(async font => ({
    path: path.resolve(font),
    sha256: createHash("sha256").update(await readFile(font)).digest("hex"),
  })),
]);
const fixtureSources = new Map();
for (const fixture of manifest.fixtures) {
  for (const relativeSource of [fixture.source ?? reference.source, fixture.noon_source].filter(Boolean)) {
    if (!fixtureSources.has(relativeSource)) {
      fixtureSources.set(relativeSource, await readFile(path.join(repoRoot, relativeSource), "utf8"));
    }
  }
}

function fixtureSourceFor(fixture) {
  const relativeSource = fixture.source ?? reference.source;
  const source = fixtureSources.get(relativeSource);
  assert.ok(source, `${fixture.id}: missing canonical source ${relativeSource}`);
  return source;
}

function fixtureSourcePathFor(fixture) {
  return path.join(repoRoot, fixture.source ?? reference.source);
}
const artifactRoot = path.resolve(
  repoRoot,
  process.env.NOON_MANIM_RASTER_ARTIFACTS ?? "manim-raster-artifacts",
);
const semanticRoot = path.join(artifactRoot, "semantic");
const manimSemanticPath = path.join(semanticRoot, "manim-all-frames.json");
const port = Number(process.env.NOON_MANIM_RASTER_PORT ?? "4191");
const enforce = process.env.NOON_MANIM_RASTER_ENFORCE === "1";
const backends = (process.env.NOON_MANIM_RASTER_BACKENDS ?? "webgpu,webgl")
  .split(",")
  .map((value) => value.trim())
  .filter(Boolean);

for (const backend of backends) {
  assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend ${backend}`);
}
assert.equal(reference.version, "0.21.0", "raster oracle must stay pinned to ManimCE 0.21.0");
assert.equal(reference.renderer, "cairo", "initial raster oracle is defined against Cairo");
for (const fixture of manifest.fixtures) {
  assert.ok(
    fixtureSourceFor(fixture).includes("from manim import *"),
    `${fixture.id}: canonical source must import real Manim`,
  );
}

await rm(artifactRoot, { recursive: true, force: true });
await mkdir(artifactRoot, { recursive: true });

function runChecked(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    ...options,
  });
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed (${result.status})\n${result.stdout}\n${result.stderr}`,
    );
  }
  return result;
}

function verifyManimVersion() {
  const version = runChecked("python3", ["-m", "manim", "--version"]);
  const output = `${version.stdout}\n${version.stderr}`;
  assert.ok(
    output.includes(reference.version),
    `expected ManimCE ${reference.version}; got ${output.trim()}`,
  );
}

async function walkFiles(root) {
  const entries = await readdir(root, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const entryPath = path.join(root, entry.name);
    if (entry.isDirectory()) files.push(...(await walkFiles(entryPath)));
    else files.push(entryPath);
  }
  return files;
}

async function findPngFrames(root, scene) {
  const escapedScene = scene.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(`^${escapedScene}(\\d+)\\.png$`);
  const frames = (await walkFiles(root))
    .map((file) => ({ file, match: path.basename(file).match(pattern) }))
    .filter(({ match }) => match)
    .sort((left, right) => Number(left.match[1]) - Number(right.match[1]))
    .map(({ file }) => file);
  assert.ok(frames.length > 0, `${scene}: expected Manim PNG frames under ${root}`);
  return frames;
}

function sampleFrames(frameTimes, fixture) {
  return sampleRasterFrames(frameTimes, manifest.sample_fractions, fixture?.sample_times);
}

async function assertSpatialMeshOracle(semantic, samples) {
  const byIndex = new Map(semantic.frames.map(frame => [frame.frame_index, frame]));
  for (const sample of samples) {
    const frame = byIndex.get(sample.frameIndex);
    assert.ok(frame?.oracle, `spatial-mesh-depth: missing numeric Manim state at ${sample.time}`);
    const { camera, red_center: red, blue_center: blue } = frame.oracle;
    assert.ok(Math.abs(red[2] - (1 - sample.time)) < 1e-6,
      `spatial-mesh-depth: red world z at ${sample.time} must follow 1-t; got ${red[2]}`);
    assert.ok(Math.abs(blue[2]) < 1e-6,
      `spatial-mesh-depth: blue world z must stay at zero; got ${blue[2]}`);
    assert.ok(Math.abs(camera.frame_center[0] - sample.time * 0.125) < 1e-6,
      `spatial-mesh-depth: camera center x at ${sample.time}; got ${camera.frame_center[0]}`);
    assert.ok(Math.abs(camera.phi) < 1e-6 && Math.abs(Math.cos(camera.theta)) < 1e-6,
      "spatial-mesh-depth: camera must face the XY triangle plane");
    assert.ok(Math.abs(camera.focal_distance - 5) < 1e-6,
      "spatial-mesh-depth: Manim focal distance must match the authored camera");
    assert.ok(Math.abs(camera.zoom - 4 / (5 * Math.tan(0.5))) < 1e-6,
      "spatial-mesh-depth: Manim zoom must match the perspective frame height");

    if (Math.abs(sample.time - 0.5) < 1e-9 || sample.time >= 1.5) {
      const png = PNG.sync.read(await readFile(sample.referencePath));
      const offset = (Math.floor(png.height / 2) * png.width + Math.floor(png.width / 2)) * 4;
      const [redPixel, , bluePixel] = png.data.subarray(offset, offset + 3);
      if (Math.abs(sample.time - 0.5) < 1e-9) {
        assert.ok(redPixel > bluePixel * 1.5,
          `spatial-mesh-depth: nearer red triangle must win Manim depth at t=${sample.time}`);
      } else {
        assert.ok(bluePixel > redPixel * 1.5,
          `spatial-mesh-depth: rear blue triangle must win after the red depth crossing at t=${sample.time}`);
      }
    }
  }
}

function assertSpatialCameraLabelsOracle(semantic, samples) {
  const byIndex = new Map(semantic.frames.map(frame => [frame.frame_index, frame]));
  const expectedLabels = {
    world: { center: [-3.2, 2.6, 0.1], width: 1.8, height: 0.28 },
    formula: { center: [2.2, 2.6, 0.1], width: 0.8, height: 0.55 },
    fixed_left: { center: [-1, -2, 0.3], width: 1.2, height: 0.24 },
    fixed_right: { center: [1, -2, -0.3], width: 1.2, height: 0.24 },
    hud: { center: [-3.4, -3.4, 0], width: 1.1, height: 0.22 },
  };
  const expectedCamera = {
    phi: [0.6, 0.8], theta: [-1.2, -0.1], gamma: [0, 0.2],
    focal_distance: [5, 5], zoom: [1, 1.1],
    frame_center: [[0, 0, 0], [0.3, 0, 0]],
  };
  for (const sample of samples) {
    const frame = byIndex.get(sample.frameIndex);
    const oracle = frame?.oracle;
    assert.ok(oracle, `spatial-camera-labels: missing numeric Manim state at ${sample.time}`);
    const progress = Math.min(1, Math.max(0, sample.time));
    const interpolate = (start, end) => start + (end - start) * progress;
    for (const key of ["phi", "theta", "gamma", "focal_distance", "zoom"]) {
      const expected = interpolate(...expectedCamera[key]);
      assert.ok(Math.abs(oracle.camera[key] - expected) < 1e-6,
        `spatial-camera-labels: camera ${key} at ${sample.time}; got ${oracle.camera[key]}, expected ${expected}`);
    }
    expectedCamera.frame_center[0].forEach((start, axis) => {
      const expected = interpolate(start, expectedCamera.frame_center[1][axis]);
      assert.ok(Math.abs(oracle.camera.frame_center[axis] - expected) < 1e-6,
        `spatial-camera-labels: camera frame center axis ${axis} at ${sample.time}`);
    });
    for (const [name, expected] of Object.entries(expectedLabels)) {
      const actual = oracle.labels[name];
      assert.ok(actual, `spatial-camera-labels: missing ${name} geometry oracle`);
      for (let axis = 0; axis < 3; axis += 1) {
        assert.ok(Math.abs(actual.center[axis] - expected.center[axis]) < 1e-6,
          `spatial-camera-labels: ${name} center axis ${axis} at ${sample.time}`);
      }
      assert.ok(Math.abs(actual.width - expected.width) < 1e-6,
        `spatial-camera-labels: ${name} authored width`);
      assert.ok(Math.abs(actual.height - expected.height) < 1e-6,
        `spatial-camera-labels: ${name} authored height`);
    }
    const familyCenter = oracle.labels.fixed_family_center;
    for (const [axis, expected] of [0, -2, 0].entries()) {
      assert.ok(Math.abs(familyCenter[axis] - expected) < 1e-6,
        `spatial-camera-labels: fixed-orientation family center axis ${axis}`);
    }
  }
}

function assertSpatialCameraLabelsFrame(debugFrame, time) {
  const allSpatialRows = debugFrame.objects.filter(object => object.spatial);
  const rows = allSpatialRows.filter(object => object.present);
  const domains = new Map();
  for (const object of rows) {
    const domain = object.spatial.composition_domain;
    domains.set(domain, (domains.get(domain) ?? 0) + 1);
  }
  const fixed = rows.filter(object => object.spatial.composition_domain === "fixed_orientation");
  const hud = rows.filter(object => object.spatial.composition_domain === "fixed_frame");
  assert.equal(fixed.length, 2,
    `spatial-camera-labels: both retained fixed-orientation labels must stay present at ${time}`);
  assert.equal(hud.length, 1,
    `spatial-camera-labels: retained fixed-frame HUD must stay present at ${time}`);
  assert.ok((domains.get("world") ?? 0) >= 3,
    `spatial-camera-labels: background and world labels must stay in the world lane at ${time}`);
  const centers = fixed.map(object => object.spatial.fixed_orientation_center);
  assert.ok(centers.every(Boolean), `spatial-camera-labels: missing shared family anchor at ${time}`);
  for (const axis of [0, 1, 2]) {
    assert.ok(centers.every(center => Math.abs(center[axis] - [0, -2, 0][axis]) < 1e-5),
      `spatial-camera-labels: common fixed-family center axis ${axis} at ${time}`);
  }
  const cameraRows = allSpatialRows.filter(object => object.spatial.camera_projection !== null);
  assert.equal(cameraRows.length, 1, `spatial-camera-labels: one retained camera row at ${time}`);
  assert.equal(cameraRows[0].spatial.camera_projection.near, 0.1);
  assert.equal(cameraRows[0].spatial.camera_projection.far, 100);
}

function assertSpatialCameraHudPixels(buffer, context) {
  const png = PNG.sync.read(buffer);
  const pixelsPerWorldUnit = png.height / 8;
  const centerX = png.width / 2 - 3.4 * pixelsPerWorldUnit;
  const centerY = png.height / 2 + 3.4 * pixelsPerWorldUnit;
  const halfWidth = 1.1 * pixelsPerWorldUnit / 2 + 3;
  const halfHeight = 0.22 * pixelsPerWorldUnit / 2 + 3;
  let visibleYellowPixels = 0;
  for (let y = Math.max(0, Math.floor(centerY - halfHeight));
       y <= Math.min(png.height - 1, Math.ceil(centerY + halfHeight)); y += 1) {
    for (let x = Math.max(0, Math.floor(centerX - halfWidth));
         x <= Math.min(png.width - 1, Math.ceil(centerX + halfWidth)); x += 1) {
      const offset = (y * png.width + x) * 4;
      const [red, green, blue, alpha] = png.data.subarray(offset, offset + 4);
      if (alpha > 0 && red > 100 && green > 90 && blue < Math.min(red, green) * 0.65) {
        visibleYellowPixels += 1;
      }
    }
  }
  assert.ok(visibleYellowPixels > 0,
    `${context}: expected the fixed-frame yellow label inside its authored bounds`);
}

async function assertVectorSpaceOracle(semantic, samples) {
  const byIndex = new Map(semantic.frames.map(frame => [frame.frame_index, frame]));
  // The ordinary one-second GrowArrow entrance precedes the three-second
  // matrix transform. The initial moving vector is collapsed; use authored
  // endpoints rather than treating that frame as the matrix's starting pose.
  const authored = { basis_i: [1, 0], basis_j: [0, 1], moving_vector: [2, 1] };
  for (const sample of samples) {
    const oracle = byIndex.get(sample.frameIndex)?.oracle;
    assert.ok(oracle, `vector-space-lts: missing numeric Manim state at ${sample.time}`);
    // Pinned Manim smooth uses a normalized sigmoid with inflection 10.
    const sigmoid = value => 1 / (1 + Math.exp(-value));
    const edge = sigmoid(-5);
    const smooth = progress => Math.min(1, Math.max(0,
      (sigmoid(10 * (progress - 0.5)) - edge) / (1 - 2 * edge)));
    const matrixProgress = smooth(Math.min(1, Math.max(0, (sample.time - 1) / 3)));
    const checkEndpoint = (name, target) => {
      const vector = oracle[name];
      const initial = authored[name];
      assert.ok(vector, `vector-space-lts: missing ${name} endpoints`);
      assert.ok(Math.abs(vector.start[0]) < 1e-6 && Math.abs(vector.start[1]) < 1e-6,
        `vector-space-lts: ${name} remains anchored at the origin`);
      const expected = name === "moving_vector" && sample.time < 1
        ? initial.map(value => value * smooth(sample.time))
        : initial.map((value, axis) => value + (target[axis] - value) * matrixProgress);
      assert.ok(Math.abs(vector.end[0] - expected[0]) < 1e-6,
        `vector-space-lts: ${name} x endpoint matches grow/matrix interpolation`);
      assert.ok(Math.abs(vector.end[1] - expected[1]) < 1e-6,
        `vector-space-lts: ${name} y endpoint matches grow/matrix interpolation`);
    };
    checkEndpoint("basis_i", [0, 1]);
    checkEndpoint("basis_j", [1, 0]);
    checkEndpoint("moving_vector", [1, 2]);
  }
}

function assertVectorSpaceFrame(frame, oracle, context) {
  // These fixture colors identify the three ordinary Arrow families without
  // depending on target-specific semantic IDs or leaf ordering.
  for (const name of ["basis_i", "basis_j", "moving_vector"]) {
    const expected = oracle[name];
    const matchesColor = paint => paint && [paint.red, paint.green, paint.blue]
      .every((channel, index) => Math.abs(channel - expected.color[index]) < 1e-6);
    const rows = frame.objects.filter(row => row.present
      && (matchesColor(row.stroke) || matchesColor(row.fill)));
    assert.equal(rows.length, 2, `${context}: ${name} retains its shaft and tip`);
    for (const [edge, operation] of [["min", Math.min], ["max", Math.max]]) {
      for (let axis = 0; axis < 2; axis += 1) {
        const actual = operation(...rows.map(row => row.bounds[edge][axis]));
        assert.ok(Math.abs(actual - expected.bounds[edge][axis]) < 1e-5,
          `${context}: ${name} ${edge}[${axis}] ${actual} differs from Manim ${expected.bounds[edge][axis]}`);
      }
    }
  }
}

async function renderManimReferences() {
  verifyManimVersion();
  await mkdir(semanticRoot, { recursive: true });
  runChecked("python3", [
    path.join("scripts", "manim-reference-run.py"),
    path.join("scripts", "manim-raster-semantic-reference.py"),
    "--manifest",
    manifestPath,
    "--output",
    manimSemanticPath,
  ]);
  const semantic = JSON.parse(await readFile(manimSemanticPath, "utf8"));
  assert.equal(semantic.manim_version, reference.version, "raster semantic Manim version");
  assert.equal(semantic.frame_rate, reference.frame_rate, "raster semantic frame rate");
  const semanticByFixture = new Map(
    semantic.fixtures.map((fixture) => [fixture.id, fixture]),
  );

  const results = new Map();
  for (const fixture of manifest.fixtures) {
    const mediaDir = path.join(artifactRoot, "manim-media", fixture.id);
    const frameDir = path.join(artifactRoot, "reference", fixture.id);
    await mkdir(mediaDir, { recursive: true });
    await mkdir(frameDir, { recursive: true });

    runChecked("python3", [
      path.join("scripts", "manim-reference-run.py"),
      "-m",
      "manim",
      "--renderer=cairo",
      "--disable_caching",
      "--format=png",
      "--media_dir",
      mediaDir,
      "-r",
      `${reference.pixel_width},${reference.pixel_height}`,
      "--fps",
      String(reference.frame_rate),
      fixtureSourcePathFor(fixture),
      fixture.scene,
    ]);

    const frameFiles = await findPngFrames(mediaDir, fixture.scene);
    const semanticFixture = semanticByFixture.get(fixture.id);
    assert.ok(semanticFixture, `${fixture.id}: missing semantic reference fixture`);
    assert.equal(
      semanticFixture.frame_count,
      frameFiles.length,
      `${fixture.id}: semantic/PNG Manim frame count`,
    );
    const frameTimes = semanticFixture.frames.map((frame) => Number(frame.time));
    const firstFrame = PNG.sync.read(await readFile(frameFiles[0]));
    const logicalDuration = Number(fixture.expected_duration);
    assert.ok(Number.isFinite(logicalDuration) && logicalDuration >= 0, `${fixture.id}: logical duration`);
    const frames = {
      frameCount: frameFiles.length,
      frameRate: reference.frame_rate,
      duration: logicalDuration,
      materializedFrameSpan: frameFiles.length / reference.frame_rate,
      width: firstFrame.width,
      height: firstFrame.height,
      format: "png-sequence",
    };
    assert.equal(frames.width, reference.pixel_width, `${fixture.id}: Manim reference width`);
    assert.equal(frames.height, reference.pixel_height, `${fixture.id}: Manim reference height`);

    const samples = sampleFrames(frameTimes, fixture);
    for (const sample of samples) {
      const outputPath = path.join(frameDir, `${sample.label}.png`);
      const image = await readFile(frameFiles[sample.frameIndex]);
      if (fixture.id.startsWith("spatial-camera-labels-")) {
        assertSpatialCameraHudPixels(image, `${fixture.id} Manim reference at ${sample.time}`);
      }
      await writeFile(outputPath, image);
      sample.referencePath = outputPath;
      if (fixture.id.startsWith("vector-space-lts-")) {
        sample.vectorOracle = semanticFixture.frames[sample.frameIndex].oracle;
      }
    }
    if (fixture.id === "spatial-mesh-depth") {
      await assertSpatialMeshOracle(semanticFixture, samples);
    }
    if (fixture.id === "spatial-camera-labels-direct" || fixture.id === "spatial-camera-labels-worker") {
      assertSpatialCameraLabelsOracle(semanticFixture, samples);
    }
    if (fixture.id === "vector-space-lts-direct" || fixture.id === "vector-space-lts-worker") {
      await assertVectorSpaceOracle(semanticFixture, samples);
    }
    results.set(fixture.id, { fixture, frames, frameTimes, samples });
  }
  return results;
}

async function prepareHostCapturePage(page) {
  await page.goto(`${baseUrl}/web/manim-raster-host.html`, { waitUntil: "load" });
  assert.equal(await page.evaluate(() => globalThis.crossOriginIsolated), true,
    "raster capture requires the isolated shared test server");
  await page.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonHostRaster.ready());
}

async function captureHostFixture(page, fixture, referenceResult, fixtureDir, expectedBackend) {
  if (fixture.direct_factory) {
    const loaded = await page.evaluate(async ({ factory }) => {
      const canvas = document.querySelector("#scene");
      canvas.width = 960;
      canvas.height = 540;
      const wasm = await import("./pkg/noon_web.js");
      await wasm.default();
      const renderer = await wasm[factory](canvas.transferControlToOffscreen());
      renderer.resize(960, 540);
      window.noonSpatialMeshOracle = renderer;
      renderer.directWakeDirectiveJson(0);
      let presented = false;
      for (let attempt = 0; attempt < 60; attempt += 1) {
        if (renderer.render()) { presented = true; break; }
        await new Promise(resolve => setTimeout(resolve, 10));
      }
      return { kind: "direct_typed_execution", rendererBackend: renderer.rendererBackend(),
        objectCount: renderer.objectCount(), presented, time: renderer.time() };
    }, { factory: fixture.direct_factory });
    assert.equal(loaded.kind, "direct_typed_execution", `${fixture.id}: canonical Rust/WASM scene`);
    assert.equal(loaded.rendererBackend, expectedBackend, `${fixture.id}: host renderer backend`);
    assert.equal(loaded.presented, true, `${fixture.id}: initial direct frame not presented`);
    const captures = [];
    for (const sample of referenceResult.samples) {
      const metrics = await page.evaluate(async ({ time, initialPresented }) => {
        const renderer = window.noonSpatialMeshOracle;
        let presented = initialPresented;
        if (time > 0) {
          renderer.advanceDirectRealtime(time * 1000);
          presented = false;
          for (let attempt = 0; attempt < 60; attempt += 1) {
            if (renderer.render()) { presented = true; break; }
            await new Promise(resolve => setTimeout(resolve, 10));
          }
        }
        return { error: null, presented, time: renderer.time(), objectCount: renderer.objectCount() };
      }, { time: sample.time, initialPresented: loaded.presented });
      assert.equal(metrics.error, null, `${fixture.id}: direct render error at ${sample.time}`);
      assert.equal(metrics.presented, true, `${fixture.id}: direct frame not presented at ${sample.time}`);
      assert.ok(Math.abs(Number(metrics.time) - Number(sample.time)) < 1e-9,
        `${fixture.id}: direct logical time mismatch at ${sample.time}`);
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(resolve)));
      const outputPath = path.join(fixtureDir, `${sample.label}.png`);
      await page.locator("#scene").screenshot({ path: outputPath });
      if (fixture.id === "spatial-camera-labels-direct") {
        assertSpatialCameraHudPixels(await readFile(outputPath), `${fixture.id} at ${sample.time}`);
      }
      const debugFrame = await page.evaluate(() =>
        JSON.parse(window.noonSpatialMeshOracle.debugSelectionFrameJson()));
      assert.equal(debugFrame.time, metrics.time, `${fixture.id}: diagnostic/raster time`);
      if (fixture.id === "spatial-camera-labels-direct") {
        assertSpatialCameraLabelsFrame(debugFrame, sample.time);
      }
      captures.push({ ...sample, noonPath: outputPath, metrics, debugFrame });
    }
    const completed = await page.evaluate(async (duration) => {
      const renderer = window.noonSpatialMeshOracle;
      renderer.advanceDirectRealtime(duration * 1000);
      let presented = false;
      for (let attempt = 0; attempt < 60; attempt += 1) {
        if (renderer.render()) { presented = true; break; }
        await new Promise(resolve => setTimeout(resolve, 10));
      }
      return { authoredDuration: renderer.time(), objectCount: renderer.objectCount(), presented };
    }, fixture.expected_duration);
    assert.equal(completed.authoredDuration, fixture.expected_duration,
      `${fixture.id}: typed Rust/WASM duration`);
    assert.ok(Number.isInteger(fixture.expected_object_count), `${fixture.id}: explicit typed fixture object count`);
    assert.equal(completed.objectCount, fixture.expected_object_count, `${fixture.id}: canonical object count`);
    assert.equal(completed.presented, true, `${fixture.id}: endpoint not presented`);
    const midpoint = fixture.expected_duration / 2;
    const forwardMidpoint = captures.find(capture => Math.abs(capture.time - midpoint) < 1e-9);
    assert.ok(forwardMidpoint, `${fixture.id}: requires a forward midpoint capture`);
    await page.evaluate(async (midpoint) => {
      const renderer = window.noonSpatialMeshOracle;
      renderer.seekDirect(midpoint);
      let presented = false;
      for (let attempt = 0; attempt < 60; attempt += 1) {
        if (renderer.render()) { presented = true; break; }
        await new Promise(resolve => setTimeout(resolve, 10));
      }
      if (!presented) throw new Error("direct spatial mesh reseek was not presented");
      await new Promise(resolve => requestAnimationFrame(resolve));
    }, midpoint);
    const reseekPath = path.join(fixtureDir, "reseek-midpoint.png");
    await page.locator("#scene").screenshot({ path: reseekPath });
    const forwardPixels = PNG.sync.read(await readFile(forwardMidpoint.noonPath)).data;
    const reseekPixels = PNG.sync.read(await readFile(reseekPath)).data;
    assert.deepEqual(
      reseekPixels,
      forwardPixels,
      `${fixture.id}: direct midpoint seek must reproduce its forward-sampled raster`,
    );
    return { duration: completed.authoredDuration, objectCount: completed.objectCount, captures };
  }
  const loaded = await page.evaluate(
    ({ source, loopDuration }) => window.noonHostRaster.load(source, loopDuration),
    { source: rasterFixtureSource(fixture.noon_source ? fixtureSources.get(fixture.noon_source) : fixtureSourceFor(fixture), fixture.scene, fixture), loopDuration: Math.max(1, fixture.expected_duration + 1) },
  );
  assert.equal(loaded.kind, "semantic_execution", `${fixture.id}: shared source execution`);
  assert.equal(loaded.rendererBackend, expectedBackend, `${fixture.id}: host renderer backend`);

  // Manim's final materialized frame may precede its logical endpoint. Sample
  // those exact frame times, then complete the normal source continuation to
  // verify duration/lifecycle without changing any reference screenshot.
  const frameTimes = [...referenceResult.frameTimes, fixture.expected_duration];
  const captures = [];
  for (const sample of referenceResult.samples) {
    const metrics = await page.evaluate(
      ({ frameIndex, frameTimes }) => window.noonHostRaster.renderThrough(frameIndex, frameTimes),
      { frameIndex: sample.frameIndex, frameTimes },
    );
    assert.equal(metrics.error, null, `${fixture.id}: host render error at frame ${sample.frameIndex}`);
    assert.equal(metrics.presented, true, `${fixture.id}: host frame ${sample.frameIndex} not presented`);
    assert.equal(metrics.frameIndex, sample.frameIndex, `${fixture.id}: host frame index`);
    assert.ok(Math.abs(Number(metrics.time) - Number(sample.time)) < 1e-9,
      `${fixture.id}: host logical time mismatch at frame ${sample.frameIndex}`);
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)));
    const outputPath = path.join(fixtureDir, `${sample.label}.png`);
    await page.locator("#scene").screenshot({ path: outputPath });
    if (fixture.id === "spatial-camera-labels-worker") {
      assertSpatialCameraHudPixels(await readFile(outputPath), `${fixture.id} at ${sample.time}`);
    }
    const debugFrame = await page.evaluate(() => window.noonHostRaster.debugFrame());
    assert.equal(debugFrame.time, metrics.time, `${fixture.id}: diagnostic/raster time`);
    if (fixture.id === "spatial-camera-labels-worker") {
      assertSpatialCameraLabelsFrame(debugFrame, sample.time);
    }
    captures.push({ ...sample, noonPath: outputPath, metrics, debugFrame });
  }
  const completed = await page.evaluate((times) =>
    window.noonHostRaster.renderThrough(times.length - 1, times), frameTimes);
  assert.equal(completed.authoredDuration, fixture.expected_duration,
    `${fixture.id}: shared source duration`);
  return { duration: completed.authoredDuration, objectCount: completed.objectCount, captures };
}

async function captureNoonBackend(backend, references) {
  const browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs(backend) });
  const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
  try {
    // Share the browser cache, while each source gets a fresh page and workers.
    const context = await browser.newContext({
      viewport: { width: reference.pixel_width + 40, height: reference.pixel_height + 40 },
    });
    const output = new Map();
    for (const fixture of manifest.fixtures) {
      let page = null;
      let deadline;
      try {
        const fixtureDir = path.join(artifactRoot, backend, fixture.id);
        await mkdir(fixtureDir, { recursive: true });
        page = await context.newPage();
        const result = await Promise.race([
          (async () => {
            await prepareHostCapturePage(page);
            return captureHostFixture(page, fixture, references.get(fixture.id), fixtureDir, expectedBackend);
          })(),
          new Promise((_, reject) => {
            deadline = setTimeout(() => reject(new Error("shared fixture exceeded 60 seconds")), 60_000);
          }),
        ]);
        output.set(fixture.id, result);
        console.log(`[PASS] ${fixture.id}/${backend}: shared execution completed`);
      } catch (error) {
        const detail = error instanceof Error ? error.stack ?? error.message : String(error);
        output.set(fixture.id, { error: detail });
        console.error(`[FAIL] ${fixture.id}/${backend}: ${detail}`);
      } finally {
        clearTimeout(deadline);
        await page?.close();
      }
    }
    return output;
  } finally {
    await browser.close();
  }
}

function pixelStats(buffer) {
  const png = PNG.sync.read(buffer);
  const background = dominantImageRgba(png);
  let changedPixels = 0;
  let minX = png.width;
  let minY = png.height;
  let maxX = -1;
  let maxY = -1;
  let r = 0;
  let g = 0;
  let b = 0;
  for (let offset = 0; offset < png.data.length; offset += 4) {
    const distance =
      Math.abs(png.data[offset] - background[0]) +
      Math.abs(png.data[offset + 1] - background[1]) +
      Math.abs(png.data[offset + 2] - background[2]) +
      Math.abs(png.data[offset + 3] - background[3]);
    if (distance >= 24) {
      changedPixels += 1;
      r += png.data[offset];
      g += png.data[offset + 1];
      b += png.data[offset + 2];
      const pixel = offset / 4;
      const x = pixel % png.width;
      const y = Math.floor(pixel / png.width);
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x);
      maxY = Math.max(maxY, y);
    }
  }
  const bounds = changedPixels === 0 ? null : { minX, minY, maxX, maxY };
  const centroid = bounds ? { x: (minX + maxX) / 2, y: (minY + maxY) / 2 } : null;
  return {
    width: png.width,
    height: png.height,
    background,
    changedPixels,
    bounds,
    centroid,
    foregroundMeanRgb:
      changedPixels === 0 ? null : [r / changedPixels, g / changedPixels, b / changedPixels],
  };
}

function comparePng(referenceBuffer, actualBuffer) {
  const expected = PNG.sync.read(referenceBuffer);
  const actual = PNG.sync.read(actualBuffer);
  assert.equal(actual.width, expected.width, "raster comparison width");
  assert.equal(actual.height, expected.height, "raster comparison height");
  const diff = new PNG({ width: expected.width, height: expected.height });
  let differingPixels = 0;
  let absoluteChannelError = 0;
  let maxChannelError = 0;
  for (let offset = 0; offset < expected.data.length; offset += 4) {
    let pixelError = 0;
    for (let channel = 0; channel < 4; channel += 1) {
      const error = Math.abs(expected.data[offset + channel] - actual.data[offset + channel]);
      absoluteChannelError += error;
      maxChannelError = Math.max(maxChannelError, error);
      pixelError += error;
      if (channel < 3) diff.data[offset + channel] = Math.min(255, error * 4);
    }
    diff.data[offset + 3] = 255;
    if (pixelError >= 24) differingPixels += 1;
  }
  return {
    diffBuffer: PNG.sync.write(diff),
    differingPixels,
    differingRatio: differingPixels / (expected.width * expected.height),
    meanAbsoluteChannelError: absoluteChannelError / expected.data.length,
    maxChannelError,
  };
}

function bboxDelta(referenceStats, actualStats) {
  if (!referenceStats.bounds || !actualStats.bounds) return null;
  return {
    centroidX: actualStats.centroid.x - referenceStats.centroid.x,
    centroidY: actualStats.centroid.y - referenceStats.centroid.y,
    width:
      actualStats.bounds.maxX -
      actualStats.bounds.minX -
      (referenceStats.bounds.maxX - referenceStats.bounds.minX),
    height:
      actualStats.bounds.maxY -
      actualStats.bounds.minY -
      (referenceStats.bounds.maxY - referenceStats.bounds.minY),
  };
}

async function compareAll(references, backendResults) {
  const report = {
    referenceFonts,
    reference,
    enforce,
    generatedAt: new Date().toISOString(),
    fixtures: [],
  };
  const enforcementFailures = [];
  const executionFailures = [];

  for (const fixture of manifest.fixtures) {
    const tolerance = resolveRasterTolerance(manifest, fixture);
    const referenceResult = references.get(fixture.id);
    const backendEntries = {};
    for (const backend of backends) {
      const actualResult = backendResults.get(backend).get(fixture.id);
      if (actualResult.error) {
        backendEntries[backend] = { error: actualResult.error };
        executionFailures.push(`${fixture.id}/${backend}: ${actualResult.error}`);
        continue;
      }
      const timingDelta = actualResult.duration - referenceResult.frames.duration;
      const samples = [];
      for (const capture of actualResult.captures) {
        const referenceBuffer = await readFile(capture.referencePath);
        const actualBuffer = await readFile(capture.noonPath);
        const referenceStats = pixelStats(referenceBuffer);
        const noonStats = pixelStats(actualBuffer);
        let foregroundCoverage;
        if (fixture.id.startsWith("vector-space-lts-")) {
          const oracle = capture.vectorOracle;
          assert.ok(oracle, `${fixture.id}: missing captured Manim vector oracle`);
          assertVectorSpaceFrame(capture.debugFrame, oracle, `${fixture.id}/${backend}@${capture.time}`);
          // Full-viewport thin grids differ in Cairo/WGPU antialiasing. Keep a
          // strict geometric guard so the edge-pixel budget cannot hide gaps.
          foregroundCoverage = compareForegroundCoverage(
            PNG.sync.read(referenceBuffer), PNG.sync.read(actualBuffer), {
              background: referenceStats.background,
              backgroundDistance: 24, neighborRadius: 1,
              maxMismatchFraction: 0.001, maxBoundsDelta: 1,
            },
          );
          assert.ok(foregroundCoverage.pass,
            `${fixture.id}/${backend}@${capture.time}: foreground coverage ${JSON.stringify(foregroundCoverage)}`);
        }
        const diff = comparePng(referenceBuffer, actualBuffer);
        const diffPath = path.join(
          artifactRoot,
          `diff-${backend}`,
          fixture.id,
          `${capture.label}.png`,
        );
        await mkdir(path.dirname(diffPath), { recursive: true });
        await writeFile(diffPath, diff.diffBuffer);
        const sample = {
          frameIndex: capture.frameIndex,
          time: capture.time,
          reference: referenceStats,
          noon: noonStats,
          debugFrame: capture.debugFrame,
          foregroundCoverage,
          boundsDelta: bboxDelta(referenceStats, noonStats),
          diff: {
            differingPixels: diff.differingPixels,
            differingRatio: diff.differingRatio,
            meanAbsoluteChannelError: diff.meanAbsoluteChannelError,
            maxChannelError: diff.maxChannelError,
          },
        };
        const policy = evaluateRasterTolerance({ sample, timingDelta, tolerance });
        sample.categories = policy.categories;
        sample.policy = policy;
        samples.push(sample);
        if (enforce && sample.categories.length > 0) {
          enforcementFailures.push(
            formatRasterPolicyFailure(fixture.id, backend, capture.label, policy),
          );
        }
      }
      backendEntries[backend] = {
        tolerance,
        noonDuration: actualResult.duration,
        manimVideoDuration: referenceResult.frames.duration,
        durationDelta: timingDelta,
        objectCount: actualResult.objectCount,
        samples,
      };
    }
    report.fixtures.push({
      id: fixture.id,
      scene: fixture.scene,
      expectedDuration: fixture.expected_duration,
      manim: referenceResult.frames,
      backends: backendEntries,
    });
  }

  const reportPath = path.join(artifactRoot, "report.json");
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  for (const fixture of report.fixtures) {
    for (const backend of backends) {
      const entry = fixture.backends[backend];
      if (entry.error) continue;
      const categories = [...new Set(entry.samples.flatMap((sample) => sample.categories))];
      const worstRatio = Math.max(...entry.samples.map((sample) => sample.diff.differingRatio));
      console.log(
        `${fixture.id} ${backend}: duration Δ=${entry.durationDelta.toFixed(4)}s, ` +
          `worst pixel diff=${(worstRatio * 100).toFixed(2)}%, ` +
          `categories=${categories.join("|") || "none"}`,
      );
    }
  }
  if (executionFailures.length > 0) {
    throw new Error(`Shared raster execution failures (report: ${reportPath}):\n${executionFailures.join("\n")}`);
  }
  if (enforcementFailures.length > 0) {
    throw new Error(`Manim raster parity failures:\n${enforcementFailures.join("\n")}`);
  }
  return reportPath;
}

// Ordinary synchronous construct uses the existing shared continuation host.
// Serve the real document and workers with COOP/COEP, not a second server.
const server = await serveRepository(repoRoot, port, { crossOriginIsolated: true });
const { baseUrl } = server;

try {
  const references = await renderManimReferences();
  const backendResults = new Map();
  for (const backend of backends) {
    backendResults.set(backend, await captureNoonBackend(backend, references));
  }
  const reportPath = await compareAll(references, backendResults);
  console.log(`ManimCE raster differential report: ${reportPath}`);
  if (!enforce) {
    console.log("Raster mismatches are report-only until NOON_MANIM_RASTER_ENFORCE=1 is enabled.");
  }
} finally {
  await server.close();
}
