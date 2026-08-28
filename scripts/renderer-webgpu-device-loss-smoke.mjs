import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

const { chromium } = playwright;
const { PNG } = pngjs;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_RENDERER_DEVICE_LOSS_PORT ?? "4193");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_RENDERER_DEVICE_LOSS_ARTIFACTS ??
    "browser-smoke-artifacts/renderer-device-loss",
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
  throw new Error(`Renderer device-loss server did not start: ${lastError}\n${serverOutput}`);
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

async function installDeviceCapture(page) {
  await page.addInitScript(() => {
    const state = {
      patched: false,
      patchError: null,
      devices: [],
      lost: [],
    };
    window.__noonWebGpuDeviceCapture = state;

    try {
      const gpu = navigator.gpu;
      if (!gpu || typeof gpu.requestAdapter !== "function") {
        state.patchError = "navigator.gpu.requestAdapter is unavailable";
        return;
      }
      const originalRequestAdapter = gpu.requestAdapter.bind(gpu);
      Object.defineProperty(gpu, "requestAdapter", {
        configurable: true,
        value: async (...adapterArgs) => {
          const adapter = await originalRequestAdapter(...adapterArgs);
          if (!adapter) return adapter;

          const originalRequestDevice = adapter.requestDevice.bind(adapter);
          Object.defineProperty(adapter, "requestDevice", {
            configurable: true,
            value: async (...deviceArgs) => {
              const device = await originalRequestDevice(...deviceArgs);
              const index = state.devices.length;
              state.devices.push(device);
              state.lost.push(null);
              device.lost.then((info) => {
                state.lost[index] = {
                  reason: String(info.reason ?? "unknown"),
                  message: String(info.message ?? ""),
                };
              });
              return device;
            },
          });
          return adapter;
        },
      });
      state.patched = true;
    } catch (error) {
      state.patchError = String(error);
    }
  });
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

async function destroyAndRecover(
  page,
  { deviceIndex, minimumDeviceCount, screenshotName, baselineMetrics },
) {
  await page.evaluate((index) => {
    const capture = window.__noonWebGpuDeviceCapture;
    const device = capture?.devices[index];
    if (!device) throw new Error(`captured Noon GPUDevice ${index} is unavailable`);
    device.destroy();
  }, deviceIndex);
  await page.waitForFunction(
    (index) => window.__noonWebGpuDeviceCapture?.lost[index] !== null,
    deviceIndex,
    { timeout: 10_000 },
  );

  const duringLoss = await page.evaluate(() => ({
    metrics: window.noonSmoke.metrics(),
    capture: {
      deviceCount: window.__noonWebGpuDeviceCapture.devices.length,
      lost: window.__noonWebGpuDeviceCapture.lost,
    },
  }));
  assert.equal(
    duringLoss.metrics.rendererBackend,
    "WebGPU",
    "device loss changed semantic backend identity",
  );
  assert.equal(
    duringLoss.metrics.revision,
    baselineMetrics.revision,
    "device loss reset scene revision",
  );
  assert.equal(
    duringLoss.metrics.objectCount,
    baselineMetrics.objectCount,
    "device loss reset scene objects",
  );

  let recovered = null;
  let lastRecoveryError = null;
  for (let attempt = 0; attempt < 40 && recovered === null; attempt += 1) {
    try {
      const candidate = await page.evaluate((time) => window.noonSmoke.renderAt(time), sampleTime);
      if (candidate.presented && candidate.rendererBackend === "WebGPU") {
        recovered = candidate;
      }
    } catch (error) {
      lastRecoveryError = String(error);
    }
    if (recovered === null) {
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
  }
  assert.ok(recovered, `WebGPU device did not recover: ${lastRecoveryError ?? "no frame presented"}`);
  assert.equal(recovered.error, null, "recovered renderer reported an error");
  assert.equal(recovered.revision, baselineMetrics.revision, "device recovery reset scene revision");
  assert.equal(
    recovered.objectCount,
    baselineMetrics.objectCount,
    "device recovery reset object count",
  );
  assert.ok(
    Math.abs(recovered.time - sampleTime) < 1e-6,
    "device recovery changed semantic playhead time",
  );

  const recoveryCapture = await page.evaluate((lostIndex) => {
    const capture = window.__noonWebGpuDeviceCapture;
    const replacementIndex = capture.devices.length - 1;
    return {
      deviceCount: capture.devices.length,
      lost: capture.lost,
      replacementIndex,
      replacedDevice:
        replacementIndex > lostIndex && capture.devices[lostIndex] !== capture.devices[replacementIndex],
    };
  }, deviceIndex);
  assert.ok(
    recoveryCapture.deviceCount >= minimumDeviceCount,
    `device recovery requested ${recoveryCapture.deviceCount} devices; expected at least ${minimumDeviceCount}`,
  );
  assert.equal(recoveryCapture.replacedDevice, true, "device recovery reused the lost GPUDevice");
  assert.equal(
    recoveryCapture.lost[recoveryCapture.replacementIndex],
    null,
    "replacement GPUDevice is already lost",
  );

  const screenshot = await page.locator("#scene").screenshot({
    path: path.join(artifactDir, `${screenshotName}.png`),
  });
  return { duringLoss, recovered, recoveryCapture, screenshot };
}

function changedPixelCount(leftBuffer, rightBuffer) {
  const left = PNG.sync.read(leftBuffer);
  const right = PNG.sync.read(rightBuffer);
  assert.equal(left.width, right.width, "device recovery comparison width mismatch");
  assert.equal(left.height, right.height, "device recovery comparison height mismatch");
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
  await installDeviceCapture(page);
  const initial = await waitForHarness(page);
  const captureBefore = await page.evaluate(() => ({
    patched: window.__noonWebGpuDeviceCapture?.patched ?? false,
    patchError: window.__noonWebGpuDeviceCapture?.patchError ?? null,
    deviceCount: window.__noonWebGpuDeviceCapture?.devices.length ?? 0,
    lost: window.__noonWebGpuDeviceCapture?.lost ?? [],
  }));
  assert.equal(captureBefore.patched, true, `WebGPU capture patch failed: ${captureBefore.patchError}`);
  assert.ok(captureBefore.deviceCount >= 1, "Noon's WebGPU device creation was not captured");

  const baseline = await renderAndCapture(page, "baseline");
  const firstRecovery = await destroyAndRecover(page, {
    deviceIndex: 0,
    minimumDeviceCount: 2,
    screenshotName: "recovered",
    baselineMetrics: baseline.metrics,
  });
  const secondRecovery = await destroyAndRecover(page, {
    deviceIndex: firstRecovery.recoveryCapture.replacementIndex,
    minimumDeviceCount: firstRecovery.recoveryCapture.deviceCount + 1,
    screenshotName: "recovered-second",
    baselineMetrics: baseline.metrics,
  });

  assert.ok(
    secondRecovery.recoveryCapture.replacementIndex > firstRecovery.recoveryCapture.replacementIndex,
    "second recovery did not advance the GPU device generation",
  );
  assert.equal(
    secondRecovery.recoveryCapture.lost[firstRecovery.recoveryCapture.replacementIndex]?.reason != null,
    true,
    "first replacement device did not report its loss before the second recovery",
  );

  const freshPage = await browser.newPage({ viewport: { width: 1000, height: 600 } });
  const freshErrors = collectBrowserErrors(freshPage);
  await waitForHarness(freshPage);
  const fresh = await renderAndCapture(freshPage, "fresh");

  const changedPixels = changedPixelCount(firstRecovery.screenshot, fresh.screenshot);
  assert.equal(
    changedPixels,
    0,
    `first recovered WebGPU frame differs from fresh renderer at ${changedPixels} pixels`,
  );
  const secondChangedPixels = changedPixelCount(secondRecovery.screenshot, fresh.screenshot);
  assert.equal(
    secondChangedPixels,
    0,
    `second recovered WebGPU frame differs from fresh renderer at ${secondChangedPixels} pixels`,
  );
  assert.equal(
    fresh.metrics.objectCount,
    secondRecovery.recovered.objectCount,
    "fresh/recovered object count mismatch",
  );
  assert.ok(
    Math.abs(fresh.metrics.time - secondRecovery.recovered.time) < 1e-6,
    "fresh/recovered time mismatch",
  );

  assert.deepEqual(browserErrors.pageErrors, [], "device-loss recovery emitted page errors");
  assert.deepEqual(freshErrors.pageErrors, [], "fresh comparison renderer emitted page errors");
  const unexpectedConsoleErrors = browserErrors.consoleErrors.filter(
    (message) => !/(device.*lost|device.*destroy|destroyed.*device|gpu.*lost)/i.test(message),
  );
  assert.deepEqual(
    unexpectedConsoleErrors,
    [],
    `device-loss recovery emitted unexpected console errors:\n${unexpectedConsoleErrors.join("\n")}`,
  );
  assert.deepEqual(freshErrors.consoleErrors, [], "fresh comparison renderer emitted console errors");

  await writeDiagnostics("device-loss-recovery", {
    browserVersion: browser.version(),
    initial,
    baseline: baseline.metrics,
    duringLoss: firstRecovery.duringLoss,
    recovered: firstRecovery.recovered,
    recoveryCapture: firstRecovery.recoveryCapture,
    secondRecovery: {
      duringLoss: secondRecovery.duringLoss,
      recovered: secondRecovery.recovered,
      recoveryCapture: secondRecovery.recoveryCapture,
    },
    fresh: fresh.metrics,
    captureBefore,
    changedPixels,
    secondChangedPixels,
    browserErrors,
    freshErrors,
  });
  await freshPage.close();
  await page.close();
  console.log(
    "✓ repeated WebGPU device loss advances GPU generations and preserves scene, time, and exact output",
  );
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
