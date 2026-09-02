import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_STRESS_GALLERY_SMOKE_PORT ?? "4191");
const baseUrl = `http://127.0.0.1:${port}`;
const exampleId = "manim-parity-stress-grid";

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
      const response = await fetch(`${baseUrl}/web/index.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Stress gallery smoke server did not start: ${lastError}\n${serverOutput}`);
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

  const page = await browser.newPage({ viewport: { width: 1000, height: 700 } });
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/index.html?example=${exampleId}`, { waitUntil: "load" });
  await page.waitForFunction(
    (id) =>
      window.__noonExampleGallery?.selectedExampleId === id &&
      document.querySelector("#status")?.dataset.runtimeStartup === "deferred",
    exampleId,
    { timeout: 30_000 },
  );

  await page.locator("#replace-scene").click();
  await page.waitForFunction(
    (id) => {
      const status = document.querySelector("#status");
      const patch = document.querySelector("#patch-status");
      return (
        status?.dataset.state === "error" ||
        patch?.dataset.state === "error" ||
        (patch?.dataset.state === "applied" && patch?.dataset.exampleId === id)
      );
    },
    exampleId,
    { timeout: 60_000 },
  );

  const result = await page.evaluate(() => {
    const status = document.querySelector("#status");
    const patch = document.querySelector("#patch-status");
    return {
      statusState: status?.dataset.state ?? null,
      statusText: document.querySelector("#status-text")?.textContent ?? "",
      runtimeStartup: status?.dataset.runtimeStartup ?? null,
      executionMode: status?.dataset.executionMode ?? null,
      rendererBackend: status?.dataset.rendererBackend ?? null,
      objectCount: Number(status?.dataset.objectCount ?? NaN),
      patchState: patch?.dataset.state ?? null,
      patchText: patch?.value ?? "",
      patchExampleId: patch?.dataset.exampleId ?? null,
    };
  });

  assert.notEqual(result.statusState, "error", `stress runtime failed: ${result.statusText}`);
  assert.notEqual(result.patchState, "error", `stress authoring/runtime failed: ${result.patchText}`);
  assert.equal(result.patchState, "applied", `stress scene did not apply: ${JSON.stringify(result)}`);
  assert.equal(result.patchExampleId, exampleId);
  assert.equal(result.runtimeStartup, "started-on-demand");
  assert.equal(result.executionMode, "retained");
  assert.equal(result.rendererBackend, "WebGPU");
  assert.match(result.patchText, /110 objects$/);
  assert.equal(browserErrors.length, 0, browserErrors.join("\n"));

  console.log(
    `✓ ${exampleId}: public gallery Run installed 110 objects through ${result.rendererBackend} ${result.executionMode} execution`,
  );
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
