import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

import { installWebGpuDeviceCapture, readWebGpuCapture } from "./webgpu-device-capture.mjs";

const { chromium } = playwright;
const { PNG } = pngjs;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_RENDERER_VALIDATION_ERROR_PORT ?? "4194");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_RENDERER_VALIDATION_ERROR_ARTIFACTS ??
    "browser-smoke-artifacts/renderer-validation-error",
);
const sampleTime = 0.75;

await mkdir(artifactDir, { recursive: true });

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => {
  serverOutput += chunk;
});
server.stderr.on("data", (chunk) => {
  serverOutput += chunk;
});

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
  throw new Error(`Renderer validation-error server did not start: ${lastError}\n${serverOutput}`);
}

function collectBrowserErrors(page) {
  const pageErrors = [];
  const consoleErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  return { pageErrors, consoleErrors };
}

async function waitForHarness(page) {
  await page.goto(`${baseUrl}/web/browser-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonSmoke?.state.ready === true, null, {
    timeout: 30_000,
  });
  const metrics = await page.evaluate(() => window.noonSmoke.metrics());
  assert.equal(metrics.error, null, `renderer failed to initialize: ${metrics.error}`);
  assert.equal(metrics.rendererBackend, "WebGPU", `expected WebGPU, got ${metrics.rendererBackend}`);
  return metrics;
}

async function renderAndCapture(page, name) {
  const metrics = await page.evaluate((time) => window.noonSmoke.renderAt(time), sampleTime);
  assert.equal(metrics.error, null, `${name}: renderer reported an error`);
  assert.equal(metrics.rendererBackend, "WebGPU", `${name}: backend changed unexpectedly`);
  assert.equal(metrics.presented, true, `${name}: frame was not presented`);
  assert.ok(metrics.drawCalls > 0, `${name}: frame emitted no draw calls`);
  const screenshot = await page.locator("#scene").screenshot({
    path: path.join(artifactDir, `${name}.png`),
  });
  return { metrics, screenshot };
}

async function triggerValidationError(page) {
  await page.evaluate(() => {
    const device = window.__noonWebGpuDeviceCapture?.devices[0];
    if (!device) throw new Error("captured Noon GPUDevice is unavailable");
    // WebGPU requires a non-zero usage bitmask. The browser must generate a
    // GPUValidationError for this real device operation without losing the device.
    device.createBuffer({ size: 4, usage: 0 });
  });
}

async function waitForHostGpuError(page) {
  let lastSuccess = null;
  for (let attempt = 0; attempt < 40; attempt += 1) {
    const result = await page.evaluate(async (time) => {
      try {
        return { ok: true, metrics: await window.noonSmoke.renderAt(time) };
      } catch (error) {
        return { ok: false, error: String(error) };
      }
    }, sampleTime);
    if (!result.ok) return result.error;
    lastSuccess = result.metrics;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error(
    `uncaptured WebGPU validation error never reached renderFrame; last metrics=${JSON.stringify(lastSuccess)}`,
  );
}

function changedPixelCount(leftBuffer, rightBuffer) {
  const left = PNG.sync.read(leftBuffer);
  const right = PNG.sync.read(rightBuffer);
  assert.equal(left.width, right.width, "validation comparison width mismatch");
  assert.equal(left.height, right.height, "validation comparison height mismatch");
  let changed = 0;
  for (let offset = 0; offset < left.data.length; offset += 4) {
    if (
      left.data[offset] !== right.data[offset] ||
      left.data[offset + 1] !== right.data[offset + 1] ||
      left.data[offset + 2] !== right.data[offset + 2] ||
      left.data[offset + 3] !== right.data[offset + 3]
    ) {
      changed += 1;
    }
  }
  return changed;
}

async function writeDiagnostics(name, value) {
  await writeFile(
    path.join(artifactDir, `${name}.json`),
    `${JSON.stringify(value, null, 2)}\n`,
    "utf8",
  );
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
  const browserErrors = collectBrowserErrors(page);
  await installWebGpuDeviceCapture(page);
  const initial = await waitForHarness(page);
  const captureBefore = await readWebGpuCapture(page);
  assert.equal(captureBefore.patched, true, `WebGPU capture patch failed: ${captureBefore.patchError}`);
  assert.equal(captureBefore.deviceCount, 1, "expected one initial Noon GPUDevice");

  const baseline = await renderAndCapture(page, "baseline");
  await triggerValidationError(page);
  const hostError = await waitForHostGpuError(page);
  assert.match(
    hostError,
    /WebGPU generation 1 validation error:/,
    `host GPU error lacks backend/generation context: ${hostError}`,
  );

  const afterErrorMetrics = await page.evaluate(() => window.noonSmoke.metrics());
  assert.equal(afterErrorMetrics.revision, baseline.metrics.revision, "GPU validation error reset scene revision");
  assert.equal(
    afterErrorMetrics.objectCount,
    baseline.metrics.objectCount,
    "GPU validation error reset scene objects",
  );
  assert.equal(afterErrorMetrics.rendererBackend, "WebGPU", "GPU validation error changed backend identity");

  const captureAfterError = await readWebGpuCapture(page);
  assert.equal(captureAfterError.deviceCount, 1, "validation error unexpectedly replaced the GPUDevice");
  assert.equal(captureAfterError.lost[0], null, "validation error unexpectedly lost the GPUDevice");

  const recovered = await renderAndCapture(page, "after-handled-error");
  assert.equal(recovered.metrics.revision, baseline.metrics.revision, "handled GPU error changed scene revision");
  assert.equal(recovered.metrics.objectCount, baseline.metrics.objectCount, "handled GPU error changed object count");
  assert.ok(Math.abs(recovered.metrics.time - sampleTime) < 1e-6, "handled GPU error changed semantic playhead time");

  const changedPixels = changedPixelCount(baseline.screenshot, recovered.screenshot);
  assert.equal(
    changedPixels,
    0,
    `frame after handled WebGPU validation error differs from baseline at ${changedPixels} pixels`,
  );

  assert.deepEqual(browserErrors.pageErrors, [], "handled WebGPU validation error emitted page errors");
  const unexpectedConsoleErrors = browserErrors.consoleErrors.filter(
    (message) => !/(validation|buffer.*usage|usage.*buffer)/i.test(message),
  );
  assert.deepEqual(
    unexpectedConsoleErrors,
    [],
    `handled WebGPU validation error emitted unexpected console errors:\n${unexpectedConsoleErrors.join("\n")}`,
  );

  await writeDiagnostics("validation-error", {
    browserVersion: browser.version(),
    initial,
    baseline: baseline.metrics,
    hostError,
    afterErrorMetrics,
    recovered: recovered.metrics,
    captureBefore,
    captureAfterError,
    changedPixels,
    browserErrors,
  });
  await page.close();
  console.log("✓ WebGPU validation errors propagate with context and leave the renderer usable");
} catch (error) {
  await writeDiagnostics("failure", {
    browserVersion: browser?.version() ?? null,
    error:
      error instanceof Error
        ? { name: error.name, message: error.message, stack: error.stack }
        : String(error),
    serverOutput,
  });
  throw error;
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
