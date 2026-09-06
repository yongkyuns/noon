import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

import {
  installWebGpuDeviceCaptureInWorker,
  readWorkerWebGpuCapture,
} from "./webgpu-device-capture.mjs";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_RENDERER_VALIDATION_PORT ?? "4186");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_RENDERER_VALIDATION_ARTIFACTS ??
    "browser-smoke-artifacts/renderer-webgpu-validation",
);

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
      const response = await fetch(`${baseUrl}/web/execution-worker-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`renderer validation server did not start: ${lastError}\n${serverOutput}`);
}

async function waitForWorkerCapture(worker, minimumDevices = 1, timeoutMs = 10_000) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await readWorkerWebGpuCapture(worker);
    if (last.patchError !== null) {
      throw new Error(`failed to patch render-worker WebGPU: ${last.patchError}`);
    }
    if (last.deviceCount >= minimumDevices) return last;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`Noon render worker did not expose ${minimumDevices} GPUDevice(s): ${JSON.stringify(last)}`);
}

async function renderMetrics(page, requestId) {
  return page.evaluate(
    ({ requestId }) =>
      new Promise((resolve, reject) => {
        const worker = window.__noonValidationHarness?.renderWorker;
        if (!worker) {
          reject(new Error("render worker is unavailable"));
          return;
        }
        const timeout = setTimeout(() => {
          worker.removeEventListener("message", onMessage);
          reject(new Error(`render metrics request ${requestId} timed out`));
        }, 10_000);
        function onMessage(event) {
          const message = event.data;
          if (
            message?.channel === "noon.render" &&
            message?.protocolVersion === 1 &&
            message?.requestId === requestId &&
            message?.type === "metrics"
          ) {
            clearTimeout(timeout);
            worker.removeEventListener("message", onMessage);
            resolve(message.metrics);
          }
        }
        worker.addEventListener("message", onMessage);
        worker.postMessage({
          channel: "noon.render",
          protocolVersion: 1,
          type: "metrics",
          requestId,
        });
      }),
    { requestId },
  );
}

let browser = null;
let page = null;
const pageErrors = [];
const consoleErrors = [];
const diagnostics = {};

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
  page = await browser.newPage({ viewport: { width: 900, height: 600 } });
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });

  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });
  const renderWorkerPromise = page.waitForEvent("worker", {
    predicate: (worker) => worker.url().endsWith("/web/execution-render-worker.js"),
    timeout: 10_000,
  });
  await page.evaluate(() => {
    const renderWorker = new Worker(
      new URL("./execution-render-worker.js", window.location.href),
      { type: "module", name: "noon-validation-render" },
    );
    window.__noonValidationHarness = {
      renderWorker,
      engineWorker: null,
      renderMessages: [],
      engineMessages: [],
    };
    renderWorker.addEventListener("message", (event) => {
      window.__noonValidationHarness.renderMessages.push(event.data);
    });
  });
  const renderWorker = await renderWorkerPromise;

  await installWebGpuDeviceCaptureInWorker(renderWorker);

  const engineWorkerPromise = page.waitForEvent("worker", {
    predicate: (worker) => worker.url().endsWith("/web/execution-engine-worker.js"),
    timeout: 10_000,
  });
  await page.evaluate(async () => {
    const pkg = await import("./pkg/noon_web.js");
    await pkg.default();
    const { createExplicitTransportSceneJson } = await import(
      "../scripts/explicit-transport-scene-fixture.js"
    );
    const sceneJson = createExplicitTransportSceneJson(pkg);
    const canvas = document.querySelector("#scene");
    const offscreen = canvas.transferControlToOffscreen();
    const channel = new MessageChannel();
    const harness = window.__noonValidationHarness;
    const engineWorker = new Worker(
      new URL("./execution-engine-worker.js", window.location.href),
      { type: "module", name: "noon-validation-engine" },
    );
    harness.engineWorker = engineWorker;
    engineWorker.addEventListener("message", (event) => {
      harness.engineMessages.push(event.data);
    });
    harness.renderWorker.postMessage(
      {
        channel: "noon.render",
        protocolVersion: 1,
        type: "init",
        canvas: offscreen,
        port: channel.port2,
        transportMode: "transferable",
        width: 640,
        height: 360,
      },
      [offscreen, channel.port2],
    );
    engineWorker.postMessage(
      {
        channel: "noon.engine",
        protocolVersion: 1,
        type: "init",
        port: channel.port1,
        sceneJson,
        loopDurationSeconds: 4,
        transportMode: "transferable",
        sharedSlotCapacity: 1024 * 1024,
        session: 1,
      },
      [channel.port1],
    );
  });
  await engineWorkerPromise;

  await page.waitForFunction(
    () =>
      window.__noonValidationHarness.renderMessages.some(
        (message) => message?.channel === "noon.render" && message?.type === "ready",
      ) &&
      window.__noonValidationHarness.engineMessages.some(
        (message) => message?.channel === "noon.engine" && message?.type === "ready",
      ),
    null,
    { timeout: 30_000 },
  );

  const ready = await page.evaluate(() => ({
    render: window.__noonValidationHarness.renderMessages.find(
      (message) => message?.channel === "noon.render" && message?.type === "ready",
    ),
    engine: window.__noonValidationHarness.engineMessages.find(
      (message) => message?.channel === "noon.engine" && message?.type === "ready",
    ),
  }));
  diagnostics.ready = ready;
  assert.equal(ready.render.backend, "WebGPU", `expected WebGPU, got ${ready.render.backend}`);
  assert.equal(ready.render.gpuGeneration, 1, "initial GPU generation must be explicit");

  const capturedBefore = await waitForWorkerCapture(renderWorker, 1);
  diagnostics.captureBefore = capturedBefore;
  assert.equal(capturedBefore.deviceCount, 1, "renderer must own exactly one initial GPU device");
  assert.equal(capturedBefore.lost[0], null, "initial renderer device must be healthy");

  let baseline = null;
  for (let attempt = 0; attempt < 30; attempt += 1) {
    baseline = await renderMetrics(page, 100 + attempt);
    if (baseline.presentedFrames >= 2) break;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  diagnostics.baseline = baseline;
  assert.equal(baseline.backend, "WebGPU");
  assert.equal(baseline.gpuGeneration, 1);
  assert.ok(baseline.presentedFrames >= 2, "renderer did not establish a healthy baseline");
  assert.ok(baseline.objectCount > 0, "baseline scene is unexpectedly empty");

  await renderWorker.evaluate(() => {
    const device = globalThis.__noonWebGpuDeviceCapture?.devices?.[0];
    if (!device) throw new Error("captured Noon GPUDevice is unavailable");
    // WebGPU requires a non-zero buffer usage. usage=0 therefore generates a
    // real GPUValidationError while returning an invalid buffer object; it does
    // not intentionally lose or replace the device.
    globalThis.__noonInvalidValidationBuffer = device.createBuffer({
      label: "Noon validation diagnostic oracle",
      size: 4,
      usage: 0,
    });
  });

  let diagnosticMetrics = null;
  let recoverableCount = 0;
  for (let attempt = 0; attempt < 40; attempt += 1) {
    diagnosticMetrics = await renderMetrics(page, 150 + attempt);
    recoverableCount = await page.evaluate(
      () =>
        window.__noonValidationHarness.renderMessages.filter(
          (message) => message?.channel === "noon.render" && message?.type === "recoverable_error",
        ).length,
    );
    if (recoverableCount >= 1) break;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  diagnostics.diagnosticMetrics = diagnosticMetrics;
  assert.ok(
    recoverableCount >= 1,
    `worker control boundaries never surfaced the WebGPU validation diagnostic; last metrics=${JSON.stringify(diagnosticMetrics)}`,
  );

  // Cross several additional public worker boundaries to prove the mailbox is
  // one-shot rather than merely delayed. No test-only production hook is needed.
  for (let requestId = 190; requestId < 193; requestId += 1) {
    await renderMetrics(page, requestId);
  }

  const afterDiagnostic = await page.evaluate(() => {
    const messages = window.__noonValidationHarness.renderMessages;
    return {
      recoverable: messages.filter(
        (message) => message?.channel === "noon.render" && message?.type === "recoverable_error",
      ),
      fatal: messages.filter(
        (message) => message?.channel === "noon.render" && message?.type === "error",
      ),
    };
  });
  diagnostics.afterDiagnostic = afterDiagnostic;
  assert.equal(afterDiagnostic.recoverable.length, 1, "validation must be reported exactly once");
  assert.equal(afterDiagnostic.fatal.length, 0, "validation must not become a fatal worker error");
  const diagnostic = afterDiagnostic.recoverable[0].diagnostic;
  assert.equal(diagnostic.backend, "WebGPU");
  assert.equal(diagnostic.generation, 1);
  assert.equal(diagnostic.kind, "validation");
  assert.equal(diagnostic.severity, "recoverable");
  assert.match(diagnostic.message, /validation|buffer|usage/i);

  let continued = null;
  for (let attempt = 0; attempt < 40; attempt += 1) {
    continued = await renderMetrics(page, 200 + attempt);
    if (continued.presentedFrames > baseline.presentedFrames) break;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  diagnostics.continued = continued;
  assert.equal(continued.backend, baseline.backend, "validation changed renderer backend");
  assert.equal(continued.gpuGeneration, baseline.gpuGeneration, "validation changed GPU generation");
  assert.equal(continued.objectCount, baseline.objectCount, "validation changed scene objects");
  assert.ok(
    continued.presentedFrames > baseline.presentedFrames,
    "renderer did not continue presenting after validation error",
  );

  const capturedAfter = await waitForWorkerCapture(renderWorker, 1);
  diagnostics.captureAfter = capturedAfter;
  assert.equal(capturedAfter.deviceCount, 1, "validation must not request a replacement GPUDevice");
  assert.equal(capturedAfter.lost[0], null, "validation must not lose the active GPUDevice");

  await page.locator("#scene").screenshot({ path: path.join(artifactDir, "continued.png") });

  const unexpectedConsoleErrors = consoleErrors.filter(
    (message) => !/validation|buffer|usage|WebGPU/i.test(message),
  );
  assert.deepEqual(pageErrors, [], `page errors: ${pageErrors.join("\n")}`);
  assert.deepEqual(
    unexpectedConsoleErrors,
    [],
    `unexpected console errors: ${unexpectedConsoleErrors.join("\n")}`,
  );

  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  await writeFile(
    path.join(artifactDir, "diagnostics.json"),
    `${JSON.stringify(diagnostics, null, 2)}\n`,
  );
} catch (error) {
  diagnostics.failure = String(error?.stack ?? error);
  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  diagnostics.serverOutput = serverOutput;
  await writeFile(
    path.join(artifactDir, "diagnostics.json"),
    `${JSON.stringify(diagnostics, null, 2)}\n`,
  );
  throw error;
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
