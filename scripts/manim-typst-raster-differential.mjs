import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { copyFile, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

const { chromium } = playwright;
const { PNG } = pngjs;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const manifestPath = path.join(repoRoot, "parity", "manim-v0.21", "typst-manifest.json");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const reference = manifest.reference;
const artifactRoot = path.resolve(
  repoRoot,
  process.env.NOON_MANIM_TYPST_RASTER_ARTIFACTS ?? "manim-raster-artifacts/retained-typst",
);
const port = Number(process.env.NOON_MANIM_TYPST_RASTER_PORT ?? "4197");
const baseUrl = `http://127.0.0.1:${port}`;
const backends = (process.env.NOON_MANIM_TYPST_RASTER_BACKENDS ?? "webgpu,webgl")
  .split(",")
  .map((value) => value.trim())
  .filter(Boolean);

for (const backend of backends) {
  assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend ${backend}`);
}
assert.equal(reference.version, "0.21.0", "Typst raster oracle must stay pinned to ManimCE 0.21.0");
assert.equal(reference.renderer, "cairo", "Typst raster oracle is defined against Cairo");

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

function verifyReferenceEnvironment() {
  const version = runChecked("python3", ["-m", "manim", "--version"]);
  assert.ok(
    `${version.stdout}\n${version.stderr}`.includes(reference.version),
    `expected ManimCE ${reference.version}`,
  );
  const typstVersion = runChecked("python3", [
    "-c",
    "import importlib.metadata; print(importlib.metadata.version('typst'))",
  ]);
  assert.equal(
    typstVersion.stdout.trim(),
    reference.typst_version,
    `expected Typst ${reference.typst_version}`,
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

async function renderReference(fixture) {
  const mediaDir = path.join(artifactRoot, "manim-media", fixture.id);
  const referenceDir = path.join(artifactRoot, "reference");
  await mkdir(mediaDir, { recursive: true });
  await mkdir(referenceDir, { recursive: true });
  runChecked("python3", [
    "-m",
    "manim",
    "--renderer=cairo",
    "--disable_caching",
    "-s",
    "--media_dir",
    mediaDir,
    "-r",
    `${reference.pixel_width},${reference.pixel_height}`,
    path.join(repoRoot, reference.source),
    fixture.scene,
  ]);
  const pngs = (await walkFiles(mediaDir)).filter((file) => file.endsWith(".png"));
  assert.equal(pngs.length, 1, `${fixture.id}: expected one Manim save-last-frame PNG`);
  const output = path.join(referenceDir, `${fixture.id}.png`);
  await copyFile(pngs[0], output);
  return output;
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

async function waitForServer() {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/retained-typst-raster.html`);
      if (response.ok) return;
    } catch {
      // Server is still starting.
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error("timed out waiting for retained Typst raster host");
}

async function captureFixture(browser, backend, fixture) {
  const page = await browser.newPage({
    viewport: { width: reference.pixel_width, height: reference.pixel_height },
    deviceScaleFactor: 1,
  });
  try {
    await page.goto(`${baseUrl}/web/retained-typst-raster.html`, { waitUntil: "load" });
    await page.waitForFunction(() => Boolean(window.noonRetainedTypstRaster), null, {
      timeout: 30_000,
    });
    await page.evaluate(() => window.noonRetainedTypstRaster.ready());
    const metrics = await page.evaluate(
      (config) => window.noonRetainedTypstRaster.render(config),
      {
        source: fixture.source,
        math: fixture.kind === "math-typst",
        fontSize: fixture.font_size,
        width: reference.pixel_width,
        height: reference.pixel_height,
      },
    );
    assert.equal(metrics.objectCount, 1, `${fixture.id}: retained object count`);
    assert.ok(metrics.drawCalls > 0, `${fixture.id}: retained draw calls`);
    const outputDir = path.join(artifactRoot, backend);
    await mkdir(outputDir, { recursive: true });
    const output = path.join(outputDir, `${fixture.id}.png`);
    await page.locator("#scene").screenshot({ path: output });
    return { output, metrics };
  } finally {
    await page.close();
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
  for (let offset = 0; offset < png.data.length; offset += 4) {
    const distance =
      Math.abs(png.data[offset] - background[0]) +
      Math.abs(png.data[offset + 1] - background[1]) +
      Math.abs(png.data[offset + 2] - background[2]) +
      Math.abs(png.data[offset + 3] - background[3]);
    if (distance >= 24) {
      changedPixels += 1;
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
  return {
    width: png.width,
    height: png.height,
    background,
    changedPixels,
    bounds,
    centroid: bounds ? { x: (minX + maxX) / 2, y: (minY + maxY) / 2 } : null,
  };
}

function comparePng(referenceBuffer, actualBuffer) {
  const expected = PNG.sync.read(referenceBuffer);
  const actual = PNG.sync.read(actualBuffer);
  assert.equal(actual.width, expected.width, "Typst raster comparison width");
  assert.equal(actual.height, expected.height, "Typst raster comparison height");
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
  const referenceWidth = referenceStats.bounds.maxX - referenceStats.bounds.minX;
  const referenceHeight = referenceStats.bounds.maxY - referenceStats.bounds.minY;
  const actualWidth = actualStats.bounds.maxX - actualStats.bounds.minX;
  const actualHeight = actualStats.bounds.maxY - actualStats.bounds.minY;
  return {
    centroidX: actualStats.centroid.x - referenceStats.centroid.x,
    centroidY: actualStats.centroid.y - referenceStats.centroid.y,
    width: actualWidth - referenceWidth,
    height: actualHeight - referenceHeight,
  };
}

verifyReferenceEnvironment();
await rm(artifactRoot, { recursive: true, force: true });
await mkdir(artifactRoot, { recursive: true });

const references = new Map();
for (const fixture of manifest.fixtures) {
  references.set(fixture.id, await renderReference(fixture));
}

const server = spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1"], {
  cwd: repoRoot,
  stdio: "ignore",
});

const report = {
  reference,
  policy: manifest.policy,
  fixtures: [],
};

try {
  await waitForServer();
  for (const backend of backends) {
    const browser = await chromium.launch({
      channel: "chromium",
      headless: true,
      args: browserArgs(backend),
    });
    try {
      for (const fixture of manifest.fixtures) {
        const capture = await captureFixture(browser, backend, fixture);
        const referenceBuffer = await readFile(references.get(fixture.id));
        const actualBuffer = await readFile(capture.output);
        const comparison = comparePng(referenceBuffer, actualBuffer);
        const referenceStats = pixelStats(referenceBuffer);
        const actualStats = pixelStats(actualBuffer);
        assert.ok(referenceStats.changedPixels > 0, `${fixture.id}: Manim reference is blank`);
        assert.ok(actualStats.changedPixels > 0, `${fixture.id}: Noon retained frame is blank`);
        const diffPath = path.join(artifactRoot, backend, `${fixture.id}-diff.png`);
        await writeFile(diffPath, comparison.diffBuffer);
        report.fixtures.push({
          id: fixture.id,
          scene: fixture.scene,
          backend,
          kind: fixture.kind,
          source: fixture.source,
          fontSize: fixture.font_size,
          metrics: capture.metrics,
          reference: referenceStats,
          actual: actualStats,
          bboxDelta: bboxDelta(referenceStats, actualStats),
          differingPixels: comparison.differingPixels,
          differingRatio: comparison.differingRatio,
          meanAbsoluteChannelError: comparison.meanAbsoluteChannelError,
          maxChannelError: comparison.maxChannelError,
        });
      }
    } finally {
      await browser.close();
    }
  }
} finally {
  server.kill("SIGTERM");
}

await writeFile(path.join(artifactRoot, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
for (const result of report.fixtures) {
  console.log(
    `${result.backend}/${result.id}: diff=${result.differingRatio.toFixed(6)} ` +
      `mae=${result.meanAbsoluteChannelError.toFixed(4)} bbox=${JSON.stringify(result.bboxDelta)}`,
  );
}
