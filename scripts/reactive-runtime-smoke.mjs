import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { PNG } from "pngjs";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4179;
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
      const response = await fetch(`${baseUrl}/web/manim-compat-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Reactive runtime smoke server did not start: ${lastError}\n${serverOutput}`);
}

function foregroundCentroid(buffer) {
  const image = PNG.sync.read(buffer);
  const background = [image.data[0], image.data[1], image.data[2], image.data[3]];
  let weightedX = 0;
  let weight = 0;
  let changedPixels = 0;
  for (let y = 0; y < image.height; y += 1) {
    for (let x = 0; x < image.width; x += 1) {
      const offset = (y * image.width + x) * 4;
      const distance =
        Math.abs(image.data[offset] - background[0]) +
        Math.abs(image.data[offset + 1] - background[1]) +
        Math.abs(image.data[offset + 2] - background[2]) +
        Math.abs(image.data[offset + 3] - background[3]);
      if (distance < 32) continue;
      changedPixels += 1;
      weightedX += x * distance;
      weight += distance;
    }
  }
  assert.ok(
    changedPixels > 200,
    `rendered tracker scene should contain foreground pixels; got ${changedPixels}`,
  );
  return weightedX / weight;
}

async function waitForPresentation(page) {
  await page.evaluate(
    () => new Promise((resolve) => requestAnimationFrame(() => resolve())),
  );
}

const source = `
from noon import *

class ReactiveCanvasDemo(Scene):
    def construct(self):
        circle = Circle(radius=0.55, color=BLUE)
        self.add(circle)

        progress = self.value_tracker(0.0)
        self.bind_position(circle, progress, direction=RIGHT, offset=(-2.0, 0.0, 0.0))

        opacity = self.value_tracker(1.0)
        self.bind_opacity(circle, opacity)

        self.play(progress.animate.set_value(4.0), run_time=2.0, rate_func=linear)
`;

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
  const page = await browser.newPage({ viewport: { width: 720, height: 440 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonManimCompat.ready());
  const authored = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    source,
  );

  const runtimeInfo = await page.evaluate(async (sceneDocument) => {
    const wasm = await import("./pkg/noon_web.js");
    await wasm.default();
    const canvas = document.createElement("canvas");
    canvas.id = "reactive-smoke-canvas";
    canvas.width = 640;
    canvas.height = 360;
    canvas.style.width = "640px";
    canvas.style.height = "360px";
    document.body.innerHTML = "";
    document.body.append(canvas);

    const player = await wasm.ReactiveCanvasPlayer.create(
      canvas,
      JSON.stringify(sceneDocument),
      4.0,
    );
    window.__reactivePlayer = player;
    const first = player.seek(0.0);
    if (!first) throw new Error("initial reactive frame was not presented");
    return {
      backend: player.rendererBackend(),
      objectCount: player.objectCount(),
      time: player.time(),
    };
  }, authored.document);
  assert.equal(runtimeInfo.backend, "WebGPU");
  assert.equal(runtimeInfo.objectCount, 1);
  assert.equal(runtimeInfo.time, 0);

  await waitForPresentation(page);
  const canvas = page.locator("#reactive-smoke-canvas");
  const initial = await canvas.screenshot();
  const initialX = foregroundCentroid(initial);

  const middleTime = await page.evaluate(() => {
    const player = window.__reactivePlayer;
    const second = player.seek(1.0);
    if (!second) throw new Error("advanced reactive frame was not presented");
    return player.time();
  });
  assert.equal(middleTime, 1);
  await waitForPresentation(page);

  const middle = await canvas.screenshot();
  const middleX = foregroundCentroid(middle);
  assert.ok(
    middleX - initialX > 70,
    `native tracker timeline should move the rendered circle rightward (${initialX} -> ${middleX})`,
  );

  await page.evaluate(() => {
    const player = window.__reactivePlayer;
    player.setReactiveInput(2, 0.5);
    try {
      player.setReactiveInput(0, 1.0);
      throw new Error("timeline-driven signal accepted an external write");
    } catch (error) {
      if (!String(error).includes("timeline-driven")) throw error;
    }
  });

  assert.equal(errors.length, 0, errors.join("\n"));
  console.log("reactive runtime browser smoke test passed");
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
