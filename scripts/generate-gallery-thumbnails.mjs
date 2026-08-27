import assert from "node:assert/strict";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

const { chromium } = playwright;
const { PNG } = pngjs;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const manifestPath = path.join(
  repoRoot,
  "web",
  "python",
  "examples",
  "manim_tutorial_manifest.json",
);
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const placeholder = "thumbnails/manim/exact-source.svg";
const generatedThumbnailRoot = "thumbnails/manim/generated";
const entries = manifest.entries.filter(
  (entry) => entry.status === "ready" && entry.thumbnail === placeholder,
);
assert.ok(entries.length > 0, "expected ready gallery entries using the placeholder thumbnail");

const outputRoot = path.resolve(
  repoRoot,
  process.env.NOON_GALLERY_THUMBNAIL_OUTPUT ?? "gallery-thumbnail-artifacts",
);
await mkdir(outputRoot, { recursive: true });

const port = Number(process.env.NOON_GALLERY_THUMBNAIL_PORT ?? "4198");
const baseUrl = `http://127.0.0.1:${port}`;
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
      const response = await fetch(`${baseUrl}/web/manim-raster-host.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`gallery thumbnail server did not start: ${lastError}\n${serverOutput}`);
}

function slugFor(entry) {
  return entry.id.replace(/^manim-/, "").replace(/^parity-/, "");
}

function framePlan(timeSeconds, frameRate = 30) {
  const targetTime = Number(timeSeconds ?? 0);
  assert.ok(Number.isFinite(targetTime) && targetTime >= 0, "thumbnail_time must be finite/non-negative");
  const frameIndex = Math.max(0, Math.round(targetTime * frameRate));
  const frameTimes = Array.from({ length: frameIndex + 1 }, (_, index) => index / frameRate);
  // Preserve an explicitly requested time even if a future manifest entry is not
  // exactly representable on the 30 fps Manim reference grid.
  frameTimes[frameIndex] = targetTime;
  return { frameIndex, frameTimes, targetTime };
}

function assertUsefulPoster(buffer, entry) {
  const png = PNG.sync.read(buffer);
  assert.equal(png.width, 960, `${entry.id}: poster width`);
  assert.equal(png.height, 540, `${entry.id}: poster height`);
  const first = [png.data[0], png.data[1], png.data[2], png.data[3]];
  let changed = 0;
  for (let offset = 0; offset < png.data.length; offset += 4) {
    const delta =
      Math.abs(png.data[offset] - first[0]) +
      Math.abs(png.data[offset + 1] - first[1]) +
      Math.abs(png.data[offset + 2] - first[2]) +
      Math.abs(png.data[offset + 3] - first[3]);
    if (delta >= 24) changed += 1;
  }
  assert.ok(changed >= 16, `${entry.id}: poster appears blank (${changed} changed pixels)`);
}

function remapManifest(generated) {
  const byId = new Map(generated.map((item) => [item.id, item]));
  return {
    ...manifest,
    entries: manifest.entries.map((entry) => {
      const poster = byId.get(entry.id);
      if (!poster) return entry;
      return {
        ...entry,
        thumbnail: `${generatedThumbnailRoot}/${poster.filename}`,
      };
    }),
  };
}

let browser = null;
try {
  await waitForServer();
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

  const generated = [];
  for (const entry of entries) {
    const page = await browser.newPage({ viewport: { width: 960, height: 540 } });
    const browserErrors = [];
    page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
    page.on("console", (message) => {
      if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
    });
    try {
      await page.goto(`${baseUrl}/web/manim-raster-host.html`, { waitUntil: "load" });
      await page.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
      await page.evaluate(() => window.noonHostRaster.ready());

      const source = await readFile(path.join(repoRoot, "web", entry.path), "utf8");
      const expectedDuration = Number(entry.expected_duration ?? 0);
      const loaded = await page.evaluate(
        ({ source, loopDuration }) => window.noonHostRaster.load(source, loopDuration),
        { source, loopDuration: Math.max(1, expectedDuration + 1) },
      );
      assert.ok(
        Math.abs(loaded.duration - expectedDuration) <= 1e-9,
        `${entry.id}: authored duration ${loaded.duration} != ${expectedDuration}`,
      );
      assert.ok(loaded.objectCount > 0, `${entry.id}: authored scene must contain objects`);
      assert.equal(loaded.rendererBackend, "WebGL2", `${entry.id}: deterministic poster backend`);

      const plan = framePlan(entry.thumbnail_time, 30);
      const metrics = await page.evaluate(
        ({ frameIndex, frameTimes }) => window.noonHostRaster.renderThrough(frameIndex, frameTimes),
        plan,
      );
      assert.equal(metrics.error, null, `${entry.id}: render error`);
      assert.ok(Math.abs(metrics.time - plan.targetTime) < 1e-9, `${entry.id}: thumbnail time`);
      await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)));

      const filename = `${slugFor(entry)}.png`;
      const outputPath = path.join(outputRoot, filename);
      await page.locator("#scene").screenshot({ path: outputPath, type: "png" });
      const buffer = await readFile(outputPath);
      assertUsefulPoster(buffer, entry);
      const info = await stat(outputPath);
      generated.push({
        id: entry.id,
        filename,
        bytes: info.size,
        thumbnailTime: plan.targetTime,
        objectCount: loaded.objectCount,
      });
      assert.deepEqual(browserErrors, [], `${entry.id}: browser errors\n${browserErrors.join("\n")}`);
      console.log(`[THUMBNAIL] ${entry.id} -> ${filename} @ ${plan.targetTime.toFixed(3)}s`);
    } finally {
      await page.close();
    }
  }

  await writeFile(
    path.join(outputRoot, "manifest.json"),
    `${JSON.stringify({ generated }, null, 2)}\n`,
    "utf8",
  );
  await writeFile(
    path.join(outputRoot, "manim_tutorial_manifest.json"),
    `${JSON.stringify(remapManifest(generated), null, 2)}\n`,
    "utf8",
  );
  console.log(`${generated.length}/${entries.length} gallery poster frames generated`);
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
