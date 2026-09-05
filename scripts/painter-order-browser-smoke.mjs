import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

const { chromium } = playwright;
const { PNG } = pngjs;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_PAINTER_ORDER_PORT ?? "4197");
const baseUrl = `http://127.0.0.1:${port}`;

const generated = spawnSync(
  "python3",
  [
    "-c",
    [
      "import sys",
      "sys.path.insert(0, 'web/python')",
      "namespace = {}",
      "source_path = 'web/python/examples/painter_order_overlap.py'",
      "source = open(source_path, encoding='utf-8').read()",
      "exec(compile(source, source_path, 'exec'), namespace)",
      "print(namespace['result'].to_json())",
    ].join("; "),
  ],
  { cwd: repoRoot, encoding: "utf8" },
);
if (generated.status !== 0) {
  throw new Error(`Unable to generate painter-order scene:\n${generated.stdout}\n${generated.stderr}`);
}
const sceneJson = generated.stdout.trim();
assert.ok(sceneJson.length > 0, "painter-order scene JSON is empty");

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => { serverOutput += chunk; });
server.stderr.on("data", (chunk) => { serverOutput += chunk; });

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
  throw new Error(`Painter-order server did not start: ${lastError}\n${serverOutput}`);
}

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: [
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
    ],
  });
  const page = await browser.newPage({ viewport: { width: 1000, height: 600 } });
  await page.goto(`${baseUrl}/web/browser-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonSmoke?.state.ready === true, null, {
    timeout: 30_000,
  });

  const initial = await page.evaluate(() => window.noonSmoke.metrics());
  assert.equal(initial.error, null, `WebGPU harness initialization failed: ${initial.error}`);
  assert.equal(initial.rendererBackend, "WebGPU", "painter-order smoke must exercise WebGPU");

  const loaded = await page.evaluate((json) => window.noonSmoke.loadScene(json), sceneJson);
  assert.equal(loaded.objectCount, 3, "painter-order fixture must contain exactly three objects");
  const metrics = await page.evaluate(() => window.noonSmoke.renderAt(0.5));
  assert.equal(metrics.error, null, `painter-order runtime error: ${metrics.error}`);
  assert.ok(metrics.drawCalls >= 3, "mixed painter-order fixture should cross renderer pipelines");

  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => resolve())));
  const screenshot = await page.locator("#scene").screenshot();
  const png = PNG.sync.read(screenshot);
  const centerX = Math.floor(png.width / 2);
  const centerY = Math.floor(png.height / 2);
  const samples = [];
  for (let dy = -2; dy <= 2; dy += 1) {
    for (let dx = -2; dx <= 2; dx += 1) {
      const offset = ((centerY + dy) * png.width + centerX + dx) * 4;
      samples.push([png.data[offset], png.data[offset + 1], png.data[offset + 2]]);
    }
  }
  const mean = samples.reduce(
    (sum, rgb) => [sum[0] + rgb[0], sum[1] + rgb[1], sum[2] + rgb[2]],
    [0, 0, 0],
  ).map((value) => value / samples.length);
  const [red, green, blue] = mean;
  assert.ok(
    green > red + 25 && green > blue + 25,
    `center pixel is not green-top painter order: rgb=${mean.map((value) => value.toFixed(1)).join(",")}`,
  );
  console.log(
    `WebGPU painter-order pixel oracle passed: center rgb=${mean.map((value) => value.toFixed(1)).join(",")}`,
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}

// Keep a compact sustained WebGPU profiling regression in the existing browser
// gate. This is a correctness/lifetime oracle, not a performance result, so pin
// its headless Linux backend to the same deterministic SwiftShader/Vulkan stack
// used by the dedicated WebGPU recovery tests. Timestamp queries are not exposed
// by ExecutionCanvasRenderer yet; perf-profile reports that capability explicitly.
const sustained = spawnSync(process.execPath, ["scripts/perf-profile.mjs"], {
  cwd: repoRoot,
  encoding: "utf8",
  env: {
    ...process.env,
    NOON_PERF_BACKEND: "webgpu",
    NOON_PERF_FORCE_SOFTWARE_WEBGPU: "1",
    NOON_PERF_COUNTS: "10000",
    NOON_PERF_LAYOUTS: "fixed",
    NOON_PERF_WARMUP: "8",
    NOON_PERF_FRAMES: "64",
    NOON_PERF_PORT: "4188",
  },
});
if (sustained.status !== 0) {
  throw new Error(
    `Sustained WebGPU profiling failed:\n${sustained.stdout}\n${sustained.stderr}`,
  );
}
console.log("Sustained WebGPU profiler regression passed (64 measured frames)");
