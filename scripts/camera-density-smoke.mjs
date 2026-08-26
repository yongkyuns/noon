import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

const { chromium } = playwright;
const { PNG } = pngjs;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_CAMERA_DENSITY_PORT ?? "4194");
const baseUrl = `http://127.0.0.1:${port}`;
const backendMode = process.env.NOON_BROWSER_SMOKE_BACKEND ?? "webgpu";
const expectedBackend = backendMode === "webgpu" ? "WebGPU" : "WebGL2";
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_CAMERA_DENSITY_ARTIFACTS ?? `browser-smoke-artifacts/${backendMode}/camera-density`,
);
assert.ok(
  backendMode === "webgpu" || backendMode === "webgl",
  `unknown camera-density backend: ${backendMode}`,
);
await mkdir(artifactDir, { recursive: true });

const sceneJson = JSON.stringify({
  version: 1,
  objects: [
    {
      id: 0,
      geometry: { rectangle: { size: { x: 2.0, y: 1.0 } } },
      transform: {
        translation: { x: 1.25, y: -0.75 },
        rotation: Math.PI / 6,
        scale: { x: 1.0, y: 1.0 },
      },
      style: {
        fill: { red: 1.0, green: 1.0, blue: 1.0, alpha: 1.0 },
        stroke: null,
        stroke_width: 0.0,
        stroke_width_mode: "screen_space",
        stroke_join: "miter",
        stroke_cap: "butt",
        opacity: 1.0,
      },
    },
  ],
  tracks: [],
});

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
  throw new Error(`camera-density server did not start: ${lastError}\n${serverOutput}`);
}

function browserArgs() {
  if (backendMode === "webgpu") {
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

function visibleStats(buffer) {
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
    if (distance < 32) continue;
    changedPixels += 1;
    const pixel = offset / 4;
    const x = pixel % png.width;
    const y = Math.floor(pixel / png.width);
    minX = Math.min(minX, x);
    minY = Math.min(minY, y);
    maxX = Math.max(maxX, x);
    maxY = Math.max(maxY, y);
  }
  assert.ok(changedPixels > 100, "density probe must render a visible object");
  return {
    width: png.width,
    height: png.height,
    changedPixels,
    bounds: { minX, minY, maxX, maxY },
    centroid: { x: (minX + maxX) / 2, y: (minY + maxY) / 2 },
  };
}

function boundDelta(left, right) {
  return Math.max(
    Math.abs(left.bounds.minX - right.bounds.minX),
    Math.abs(left.bounds.minY - right.bounds.minY),
    Math.abs(left.bounds.maxX - right.bounds.maxX),
    Math.abs(left.bounds.maxY - right.bounds.maxY),
  );
}

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs() });
  const page = await browser.newPage({ viewport: { width: 1000, height: 600 } });
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/browser-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonSmoke?.state.ready === true, null, {
    timeout: 30_000,
  });
  const initial = await page.evaluate(() => window.noonSmoke.metrics());
  assert.equal(initial.error, null, `${backendMode}: browser smoke initialization`);
  assert.equal(initial.rendererBackend, expectedBackend, `${backendMode}: renderer backend`);
  const loaded = await page.evaluate((json) => window.noonSmoke.loadScene(json), sceneJson);
  assert.equal(loaded.objectCount, 1, "density scene object count");

  const profiles = [
    { label: "1x", width: 960, height: 540 },
    { label: "1.5x", width: 1440, height: 810 },
    { label: "2x", width: 1920, height: 1080 },
  ];
  const captures = [];
  for (const profile of profiles) {
    const resized = await page.evaluate(
      ({ width, height }) => window.noonSmoke.resizeBacking(width, height),
      profile,
    );
    assert.equal(resized.backingWidth, profile.width, `${profile.label}: backing width`);
    assert.equal(resized.backingHeight, profile.height, `${profile.label}: backing height`);
    assert.equal(resized.cssWidth, 960, `${profile.label}: CSS width remains composition viewport`);
    assert.equal(resized.cssHeight, 540, `${profile.label}: CSS height remains composition viewport`);

    const metrics = await page.evaluate(() => window.noonSmoke.renderAt(1.0));
    assert.equal(metrics.error, null, `${profile.label}: render error`);
    assert.equal(metrics.presented, true, `${profile.label}: frame presentation`);
    assert.ok(metrics.drawCalls > 0, `${profile.label}: expected draw calls`);

    const screenshot = await page.locator("#scene").screenshot({
      path: path.join(artifactDir, `${profile.label}.png`),
    });
    const stats = visibleStats(screenshot);
    assert.equal(stats.width, 960, `${profile.label}: screenshot CSS width`);
    assert.equal(stats.height, 540, `${profile.label}: screenshot CSS height`);
    captures.push({ profile, stats });
  }

  const baseline = captures[0].stats;
  for (const { profile, stats } of captures.slice(1)) {
    const bounds = boundDelta(baseline, stats);
    const centroid = Math.max(
      Math.abs(baseline.centroid.x - stats.centroid.x),
      Math.abs(baseline.centroid.y - stats.centroid.y),
    );
    assert.ok(bounds <= 1, `${profile.label}: backing density moved CSS bounds by ${bounds}px`);
    assert.ok(centroid <= 0.5, `${profile.label}: backing density moved centroid by ${centroid}px`);
  }
  assert.deepEqual(browserErrors, [], `camera-density browser errors:\n${browserErrors.join("\n")}`);
  console.log(
    `✓ ${backendMode} camera density invariant: 960×540, 1440×810, 1920×1080 backing stores preserve CSS composition`,
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
