import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

const { chromium } = playwright;
const { PNG } = pngjs;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const manifestPath = path.join(repoRoot, "parity", "manim-v0.21", "manifest.json");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const reference = manifest.reference;
const fixtureSources = new Map();
for (const fixture of manifest.fixtures) {
  const relativeSource = fixture.source ?? reference.source;
  if (!fixtureSources.has(relativeSource)) {
    fixtureSources.set(relativeSource, await readFile(path.join(repoRoot, relativeSource), "utf8"));
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
const baseUrl = `http://127.0.0.1:${port}`;
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

function sampleFrames(frameTimes) {
  const frameCount = frameTimes.length;
  assert.ok(Number.isSafeInteger(frameCount) && frameCount > 0, "invalid reference frame count");
  const previous = frameTimes.reduce((last, time, index) => {
    assert.ok(Number.isFinite(time) && time >= 0, `invalid logical time for reference frame ${index}`);
    assert.ok(time + 1e-12 >= last, `reference frame ${index} moves backwards in logical time`);
    return time;
  }, -Infinity);
  void previous;
  const indices = manifest.sample_fractions.map((fraction) =>
    Math.round((frameCount - 1) * Number(fraction)),
  );
  return [...new Set(indices)].map((frameIndex) => ({
    frameIndex,
    time: frameTimes[frameIndex],
    label: `frame-${String(frameIndex).padStart(4, "0")}`,
  }));
}

async function renderManimReferences() {
  verifyManimVersion();
  await mkdir(semanticRoot, { recursive: true });
  runChecked("python3", [
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

    const samples = sampleFrames(frameTimes);
    for (const sample of samples) {
      const outputPath = path.join(frameDir, `${sample.label}.png`);
      await writeFile(outputPath, await readFile(frameFiles[sample.frameIndex]));
      sample.referencePath = outputPath;
    }
    results.set(fixture.id, { fixture, frames, frameTimes, samples });
  }
  return results;
}

function noonSourceFor(fixture) {
  const adapted = fixtureSourceFor(fixture).replace("from manim import *", "from noon import *");
  return `${adapted}\n\nresult = ${fixture.scene}()\nresult.setup()\ntry:\n    result.construct()\nfinally:\n    result.tear_down()\n`;
}

async function authorNoonScenes() {
  const browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: ["--disable-dev-shm-usage"],
  });
  try {
    const page = await browser.newPage();
    await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
    await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
    await page.evaluate(() => window.noonManimCompat.ready());
    const scenes = new Map();
    for (const fixture of manifest.fixtures) {
      const result = await page.evaluate(
        (source) => window.noonManimCompat.run(source),
        noonSourceFor(fixture),
      );
      assert.equal(result.kind, "scene_document", `${fixture.id}: Noon authoring result kind`);
      assert.ok(result.document.objects.length > 0, `${fixture.id}: Noon scene has no objects`);
      assert.equal(result.duration, fixture.expected_duration, `${fixture.id}: authored Noon duration`);
      scenes.set(fixture.id, {
        document: result.document,
        duration: Number(result.duration),
        hasCallbacks: result.callbacks !== null,
        hasSemanticCamera: Number.isInteger(result.document.camera_object),
      });
    }
    return scenes;
  } finally {
    await browser.close();
  }
}

function browserArgs(backend) {
  if (backend === "webgpu") {
    return [
      "--enable-unsafe-webgpu",
      "--enable-unsafe-swiftshader",
      "--use-webgpu-adapter=swiftshader",
      "--use-gpu-in-tests",
      "--ignore-gpu-blocklist",
      "--enable-features=Vulkan",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--use-vulkan=swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ];
  }
  return [
    "--disable-features=WebGPU",
    "--enable-unsafe-swiftshader",
    "--ignore-gpu-blocklist",
    "--use-gl=angle",
    "--use-angle=swiftshader",
    "--disable-gpu-sandbox",
    "--disable-dev-shm-usage",
  ];
}

async function createDeterministicCapturePage(browser, expectedBackend, backend) {
  const page = await browser.newPage({
    viewport: { width: reference.pixel_width + 40, height: reference.pixel_height + 40 },
  });
  await page.goto(`${baseUrl}/web/browser-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonSmoke?.state.ready === true, null, {
    timeout: 30_000,
  });
  const initial = await page.evaluate(() => window.noonSmoke.metrics());
  assert.equal(initial.rendererBackend, expectedBackend, `${backend}: selected renderer backend`);
  return page;
}

async function createHostCapturePage(browser) {
  const page = await browser.newPage({
    viewport: { width: reference.pixel_width + 40, height: reference.pixel_height + 40 },
  });
  await page.goto(`${baseUrl}/web/manim-raster-host.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonHostRaster.ready());
  return page;
}

async function captureDeterministicFixture(
  page,
  fixture,
  authored,
  referenceResult,
  fixtureDir,
) {
  const loaded = await page.evaluate(
    (json) => window.noonSmoke.loadScene(json),
    JSON.stringify(authored.document),
  );
  assert.equal(loaded.objectCount, authored.document.objects.length, `${fixture.id}: loaded object count`);
  const captures = [];
  for (const sample of referenceResult.samples) {
    const metrics = await page.evaluate(
      (time) => window.noonSmoke.renderAt(time),
      sample.time,
    );
    assert.equal(metrics.error, null, `${fixture.id}: Noon render error at ${sample.time}`);
    assert.equal(metrics.presented, true, `${fixture.id}: frame was not presented at ${sample.time}`);
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)));
    const outputPath = path.join(fixtureDir, `${sample.label}.png`);
    await page.locator("#scene").screenshot({ path: outputPath });
    captures.push({ ...sample, noonPath: outputPath, metrics });
  }
  return {
    duration: authored.duration,
    objectCount: authored.document.objects.length,
    captures,
  };
}

async function captureHostFixture(
  page,
  fixture,
  authored,
  referenceResult,
  fixtureDir,
  expectedBackend,
  backend,
) {
  const loaded = await page.evaluate(
    ({ source, loopDuration, mode }) => window.noonHostRaster.load(source, loopDuration, { mode }),
    {
      source: noonSourceFor(fixture),
      loopDuration: Math.max(1, fixture.expected_duration + 1),
      mode: authored.hasCallbacks ? "semantic" : "document",
    },
  );
  if (authored.hasCallbacks) {
    assert.equal(loaded.kind, "semantic_execution", `${fixture.id}: canonical callback execution`);
  } else {
    assert.equal(loaded.duration, fixture.expected_duration, `${fixture.id}: host authored duration`);
  }
  assert.equal(loaded.objectCount, authored.document.objects.length, `${fixture.id}: host object count`);
  assert.equal(loaded.rendererBackend, expectedBackend, `${backend}: host renderer backend`);

  const captures = [];
  for (const sample of referenceResult.samples) {
    const metrics = await page.evaluate(
      ({ frameIndex, frameTimes }) => window.noonHostRaster.renderThrough(frameIndex, frameTimes),
      { frameIndex: sample.frameIndex, frameTimes: referenceResult.frameTimes },
    );
    assert.equal(metrics.error, null, `${fixture.id}: host render error at frame ${sample.frameIndex}`);
    assert.equal(metrics.presented, true, `${fixture.id}: host frame ${sample.frameIndex} not presented`);
    assert.equal(metrics.frameIndex, sample.frameIndex, `${fixture.id}: host frame index`);
    assert.ok(
      Math.abs(Number(metrics.time) - Number(sample.time)) < 1e-9,
      `${fixture.id}: host logical time mismatch at frame ${sample.frameIndex}`,
    );
    // Match the deterministic capture path: rendering/present submits GPU work,
    // while the browser compositor owns when that surface becomes screenshot-visible.
    // Waiting one paint prevents callback-heavy scenes from being captured mid-present.
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)));
    const outputPath = path.join(fixtureDir, `${sample.label}.png`);
    await page.locator("#scene").screenshot({ path: outputPath });
    captures.push({ ...sample, noonPath: outputPath, metrics });
  }
  return {
    duration: authored.duration,
    objectCount: authored.document.objects.length,
    captures,
  };
}

async function captureNoonBackend(backend, authoredScenes, references) {
  const browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs(backend),
  });
  const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
  try {
    const output = new Map();
    for (const fixture of manifest.fixtures) {
      let page = null;
      try {
        const authored = authoredScenes.get(fixture.id);
        const referenceResult = references.get(fixture.id);
        const fixtureDir = path.join(artifactRoot, backend, fixture.id);
        await mkdir(fixtureDir, { recursive: true });

        if (authored.hasCallbacks || authored.hasSemanticCamera) {
          page = await createHostCapturePage(browser);
          output.set(
            fixture.id,
            await captureHostFixture(
              page,
              fixture,
              authored,
              referenceResult,
              fixtureDir,
              expectedBackend,
              backend,
            ),
          );
        } else {
          page = await createDeterministicCapturePage(browser, expectedBackend, backend);
          output.set(
            fixture.id,
            await captureDeterministicFixture(page, fixture, authored, referenceResult, fixtureDir),
          );
        }
      } finally {
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
  const background = [png.data[0], png.data[1], png.data[2], png.data[3]];
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

function classify(sample, timingDelta) {
  const categories = [];
  if (Math.abs(timingDelta) > 1 / reference.frame_rate + 1e-9) categories.push("timing");
  const backgroundDelta = sample.reference.background
    .map((value, index) => Math.abs(value - sample.noon.background[index]))
    .reduce((sum, value) => sum + value, 0);
  if (backgroundDelta >= 12) categories.push("background/color-pipeline");
  if (
    sample.boundsDelta &&
    Math.max(
      Math.abs(sample.boundsDelta.centroidX),
      Math.abs(sample.boundsDelta.centroidY),
      Math.abs(sample.boundsDelta.width),
      Math.abs(sample.boundsDelta.height),
    ) > 2
  ) {
    categories.push("camera/layout/geometry");
  }
  if (sample.diff.differingRatio > 0.02) categories.push("raster/style/animation-state");
  return [...new Set(categories)];
}

async function compareAll(references, backendResults) {
  const report = {
    reference,
    enforce,
    generatedAt: new Date().toISOString(),
    fixtures: [],
  };
  const enforcementFailures = [];

  for (const fixture of manifest.fixtures) {
    const referenceResult = references.get(fixture.id);
    const backendEntries = {};
    for (const backend of backends) {
      const actualResult = backendResults.get(backend).get(fixture.id);
      const timingDelta = actualResult.duration - referenceResult.frames.duration;
      const samples = [];
      for (const capture of actualResult.captures) {
        const referenceBuffer = await readFile(capture.referencePath);
        const actualBuffer = await readFile(capture.noonPath);
        const referenceStats = pixelStats(referenceBuffer);
        const noonStats = pixelStats(actualBuffer);
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
          boundsDelta: bboxDelta(referenceStats, noonStats),
          diff: {
            differingPixels: diff.differingPixels,
            differingRatio: diff.differingRatio,
            meanAbsoluteChannelError: diff.meanAbsoluteChannelError,
            maxChannelError: diff.maxChannelError,
          },
        };
        sample.categories = classify(sample, timingDelta);
        samples.push(sample);
        if (enforce && sample.categories.length > 0) {
          enforcementFailures.push(
            `${fixture.id}/${backend}/${capture.label}: ${sample.categories.join(", ")}`,
          );
        }
      }
      backendEntries[backend] = {
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
      const categories = [...new Set(entry.samples.flatMap((sample) => sample.categories))];
      const worstRatio = Math.max(...entry.samples.map((sample) => sample.diff.differingRatio));
      console.log(
        `${fixture.id} ${backend}: duration Δ=${entry.durationDelta.toFixed(4)}s, ` +
          `worst pixel diff=${(worstRatio * 100).toFixed(2)}%, ` +
          `categories=${categories.join("|") || "none"}`,
      );
    }
  }
  if (enforcementFailures.length > 0) {
    throw new Error(`Manim raster parity failures:\n${enforcementFailures.join("\n")}`);
  }
  return reportPath;
}

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/browser-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Manim raster server did not start: ${lastError}\n${serverOutput}`);
}

try {
  const references = await renderManimReferences();
  await waitForServer();
  const authoredScenes = await authorNoonScenes();
  const backendResults = new Map();
  for (const backend of backends) {
    backendResults.set(backend, await captureNoonBackend(backend, authoredScenes, references));
  }
  const reportPath = await compareAll(references, backendResults);
  console.log(`ManimCE raster differential report: ${reportPath}`);
  if (!enforce) {
    console.log("Raster mismatches are report-only until NOON_MANIM_RASTER_ENFORCE=1 is enabled.");
  }
} finally {
  server.kill("SIGTERM");
}
