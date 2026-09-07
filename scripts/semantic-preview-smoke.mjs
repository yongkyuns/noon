// Trusted CI fixtures only. This is not a sandbox or an agent-facing CLI.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawn, execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { once } from "node:events";
import { chromium } from "playwright";
import pngjs from "pngjs";
const { PNG } = pngjs;

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const artifacts = path.join(root, "browser-smoke-artifacts/semantic-preview");
const port = 4187;
const url = `http://127.0.0.1:${port}/manim-raster-host.html`;
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const server = spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", path.join(root, "web")], {
  stdio: ["ignore", "ignore", "pipe"],
});
let serverError = "";
server.stderr.on("data", (chunk) => { serverError = (serverError + chunk).slice(-4096); });
server.on("error", (error) => { serverError = String(error); });
let browser;
const observations = [];

async function pageReady() {
  const page = await browser.newPage({ viewport: { width: 960, height: 540 }, deviceScaleFactor: 1 });
  page.setDefaultTimeout(30_000);
  await page.goto(url);
  await page.waitForFunction(() => window.noonHostRaster !== undefined);
  await page.evaluate(() => window.noonHostRaster.ready());
  return page;
}

function foreground(bytes) {
  const png = PNG.sync.read(bytes);
  const background = [...png.data.subarray(0, 3)];
  const distance = (offset) => background.reduce((sum, value, channel) => sum + Math.abs(value - png.data[offset + channel]), 0);
  let minX = png.width, maxX = -1, count = 0;
  for (let y = 0; y < png.height; y += 1) {
    for (let x = 0; x < png.width; x += 1) {
      if (distance((y * png.width + x) * 4) > 24) {
        count += 1;
        minX = Math.min(minX, x);
        maxX = Math.max(maxX, x);
      }
    }
  }
  const center = (Math.floor(png.height / 2) * png.width + Math.floor(png.width / 2)) * 4;
  return { count, width: maxX < minX ? 0 : maxX - minX + 1, centerDistance: distance(center) };
}

async function captureSquareToCircle(label) {
  const samples = [];
  const source = await readFile(path.join(root, "web/python/examples/manim_parity_square_to_circle.py"), "utf8");
  const page = await pageReady();
  try {
    const loaded = await page.evaluate((code) => window.noonHostRaster.load(code, 4), source);
    assert.equal(loaded.kind, "semantic_execution");
    const frameTimes = Array.from({ length: 91 }, (_, frame) => frame / 30);
    const images = new Map();
    for (const frameIndex of [0, 30, 45, 60, 90]) {
      const sample = await page.evaluate(({ frameIndex, frameTimes }) => window.noonHostRaster.renderThrough(frameIndex, frameTimes), { frameIndex, frameTimes });
      const bytes = await page.locator("#scene").screenshot();
      await writeFile(path.join(artifacts, `${label}-frame-${frameIndex}.png`), bytes);
      images.set(frameIndex, foreground(bytes));
      samples.push(sample);
      observations.push({ label, frameIndex, sample, sourceSha256: hash(source), imageSha256: hash(bytes) });
    }
    assert.equal(samples[1].objectCount, 1);
    assert.equal(samples[3].objectCount, 1);
    assert.equal(samples[4].objectCount, 0);
    const square = images.get(30), circle = images.get(60);
    assert.ok(square.count > 20 && circle.count > 20, "both shape endpoints must be visible");
    assert.ok(square.centerDistance < 12, "the initial square must remain hollow, not be the filled target circle");
    assert.ok(circle.centerDistance > 24, "the transformed circle must have its authored pink fill");
    assert.ok(square.width > circle.width * 1.2, "the rotated square must be wider than the endpoint circle");
    assert.equal(images.get(90).count, 0, "FadeOut must leave an empty final frame");
    await page.evaluate(() => window.noonHostRaster.close());
    assert.equal(await page.evaluate(() => window.noonHostRaster.status().state), "closed");
  } finally {
    await page.close();
  }
}

async function cancelRunningSource() {
  const page = await pageReady();
  try {
    await page.evaluate(() => {
      const source = "from noon import *\nclass Stuck(Scene):\n    def construct(self):\n        self.add(Circle())\n        self.wait(0.1)\n        while True:\n            pass\n";
      window.previewOpening = window.noonHostRaster.load(source, 4);
      // Install the rejection handler immediately, including for startup failure.
      window.previewOpening.catch(() => {});
    });
    await page.evaluate(() => window.previewOpening);
    await page.evaluate(() => {
      window.previewPending = window.noonHostRaster.renderThrough(6, [0, 1 / 30, 2 / 30, 0.1, 4 / 30, 5 / 30, 0.2])
        .then(() => "unexpected completion", (error) => String(error));
    });
    // The main page remains responsive while its authoring worker is stuck.
    await page.waitForTimeout(200);
    await page.evaluate(() => window.noonHostRaster.close());
    const result = await page.evaluate(() => window.previewPending);
    assert.match(result, /closed/);
    const state = await page.evaluate(() => window.noonHostRaster.status());
    assert.equal(state.state, "closed");
    assert.equal(state.sourceState, "canceled");
    observations.push({ cancellation: state });
  } finally {
    await page.close();
  }
}

try {
  await mkdir(artifacts, { recursive: true });
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (!server.pid || server.exitCode !== null || server.signalCode !== null) throw new Error(`preview test server exited: ${serverError}`);
    try { ready = (await fetch(url)).ok; } catch { /* server has not bound yet */ }
    if (ready) break;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  assert.ok(ready, `preview test server did not start: ${serverError}`);
  browser = await chromium.launch({ headless: true, args: [
    "--disable-features=WebGPU", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist",
    "--use-gl=angle", "--use-angle=swiftshader",
  ] });
  await captureSquareToCircle("before-cancel");
  await cancelRunningSource();
  // A fresh page/run must work after cancellation, with no shared session state.
  await captureSquareToCircle("after-cancel");
  console.log("Semantic preview: square/morph/circle/fade frames, worker cancellation, and fresh-run recovery passed");

} finally {
  try {
    let revision = null;
    try { revision = execFileSync("git", ["-C", root, "rev-parse", "HEAD"], { encoding: "utf8" }).trim(); }
    catch { /* A source archive has no Git revision. */ }
    await writeFile(path.join(artifacts, "observations.json"), JSON.stringify({ revision, observations }, null, 2));
  } finally {
    try { await browser?.close(); }
    finally {
      if (server.pid && server.exitCode === null && server.signalCode === null) {
        const exited = once(server, "exit");
        server.kill("SIGKILL");
        await exited;
      }
    }
  }
}
