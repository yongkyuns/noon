import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_WEBGL_HOST_DIAGNOSTIC_PORT ?? "4197");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_WEBGL_HOST_DIAGNOSTIC_ARTIFACTS ?? "webgl-host-phase-diagnostic",
);

function noonRotationUpdaterSource() {
  return readFile(
    path.join(repoRoot, "parity", "manim-v0.21", "upstream-examples", "rotation_updater.py"),
    "utf8",
  ).then((source) => {
    const adapted = source.replace("from manim import *", "from noon import *");
    return `${adapted}\n\nresult = RotationUpdater()\nresult.setup()\ntry:\n    result.construct()\nfinally:\n    result.tear_down()\n`;
  });
}

function referenceFrameTimes(frameCount) {
  const times = [0];
  let time = 0;
  for (let frame = 1; frame < frameCount; frame += 1) {
    time += 1 / 30;
    times.push(time);
  }
  return times;
}

async function waitForServer() {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${baseUrl}/web/manim-raster-host.html`);
      if (response.ok) return;
    } catch {
      // Server is still starting.
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error("timed out waiting for diagnostic web server");
}

const server = spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1"], {
  cwd: repoRoot,
  stdio: ["ignore", "ignore", "inherit"],
});

let browser = null;
try {
  await waitForServer();
  await mkdir(artifactDir, { recursive: true });
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: [
      "--disable-features=WebGPU",
      "--enable-unsafe-swiftshader",
      "--ignore-gpu-blocklist",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ],
  });
  const page = await browser.newPage({ viewport: { width: 1000, height: 580 } });
  await page.goto(`${baseUrl}/web/manim-raster-host.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonHostRaster.ready());

  const source = await noonRotationUpdaterSource();
  const loaded = await page.evaluate(
    ({ sourceText, loopDuration }) => window.noonHostRaster.load(sourceText, loopDuration),
    { sourceText: source, loopDuration: 5.5 },
  );
  assert.equal(loaded.rendererBackend, "WebGL2", "diagnostic must exercise WebGL2 fallback");
  assert.equal(loaded.duration, 4.5, "RotationUpdater authored duration");
  assert.ok(loaded.callbackSlots > 0, "RotationUpdater must use host callbacks");

  const frameTimes = referenceFrameTimes(121);
  const metrics = await page.evaluate(
    ({ targetFrame, times }) => window.noonHostRaster.renderThrough(targetFrame, times),
    { targetFrame: 90, times: frameTimes },
  );
  assert.equal(metrics.error, null, "frame 90 render error");
  assert.equal(metrics.frameIndex, 90, "frame 90 diagnostic target");
  assert.ok(metrics.phases, "frame 90 phase metrics");
  assert.equal(metrics.phases.frameIndex, 90, "phase metrics frame index");
  assert.ok(metrics.phases.hostPatch, "frame 90 host patch phase");

  const reportPath = path.join(artifactDir, "frame-0090.json");
  await writeFile(reportPath, `${JSON.stringify({ loaded, metrics }, null, 2)}\n`);
  await page.locator("#scene").screenshot({ path: path.join(artifactDir, "frame-0090.png") });
  console.log(JSON.stringify({ loaded, metrics }, null, 2));
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
