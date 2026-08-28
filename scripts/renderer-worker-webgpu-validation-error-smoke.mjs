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
const port = Number(process.env.NOON_RENDERER_WORKER_VALIDATION_PORT ?? "4195");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_RENDERER_WORKER_VALIDATION_ARTIFACTS ??
    "browser-smoke-artifacts/renderer-worker-validation-error",
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
      const response = await fetch(`${baseUrl}/web/pkg/noon_web.js`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`renderer worker validation server did not start: ${lastError}\n${serverOutput}`);
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
  throw new Error(
    `Noon render worker did not expose ${minimumDevices} GPUDevice(s): ${JSON.stringify(last)}`,
  );
}

async function requestRenderMetrics(page, requestId) {
  return page.evaluate(
    ({ requestId }) =>
      new Promise((resolve, reject) => {
        const worker = window.__noonWorkerValidation?.renderWorker;
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
            message?.type === "metrics" &&
            message?.requestId === requestId
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

async function writeDiagnostics(value) {
  await writeFile(
    path.join(artifactDir, "diagnostics.json"),
    `${JSON.stringify(value, null, 2)}\n`,
    "utf8",
  );
}

let browser = null;
const diagnostics = {};
const pageErrors = [];
const consoleErrors = [];

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

  const page = await browser.newPage({ viewport: { width: 900, height: 600 } });
  await page.route("**/favicon.ico", (route) =>
    route.fulfill({ status: 204, contentType: "image/x-icon", body: "" }),
  );
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  await page.goto(baseUrl, { waitUntil: "load" });

  const renderWorkerPromise = page.waitForEvent("worker", {
    predicate: (worker) => worker.url().endsWith("/web/execution-render-worker.js"),
    timeout: 10_000,
  });
  await page.evaluate(() => {
    document.body.innerHTML = "<canvas id='scene' width='640' height='360'></canvas>";
    const renderWorker = new Worker(
      new URL("./web/execution-render-worker.js", window.location.href),
      { type: "module", name: "noon-worker-validation-render" },
    );
    window.__noonWorkerValidation = {
      renderWorker,
      engineWorker: null,
      renderMessages: [],
      engineMessages: [],
    };
    renderWorker.addEventListener("message", (event) => {
      window.__noonWorkerValidation.renderMessages.push(event.data);
    });
  });
  const renderWorker = await renderWorkerPromise;
  await installWebGpuDeviceCaptureInWorker(renderWorker);

  const engineWorkerPromise = page.waitForEvent("worker", {
    predicate: (worker) => worker.url().endsWith("/web/execution-engine-worker.js"),
    timeout: 10_000,
  });
  await page.evaluate(async () => {
    const pkg = await import("./web/pkg/noon_web.js");
    await pkg.default();
    const sceneJson = pkg.demoSceneJson();
    const canvas = document.querySelector("#scene");
    const offscreen = canvas.transferControlToOffscreen();
    const channel = new MessageChannel();
    const harness = window.__noonWorkerValidation;
    const engineWorker = new Worker(
      new URL("./web/execution-engine-worker.js", window.location.href),
      { type: "module", name: "noon-worker-validation-engine" },
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
      window.__noonWorkerValidation.renderMessages.some(
        (message) => message?.channel === "noon.render" && message?.type === "ready",
      ) &&
      window.__noonWorkerValidation.engineMessages.some(
        (message) => message?.channel === "noon.engine" && message?.type === "ready",
      ),
    null,
    { timeout: 30_000 },
  );

  const ready = await page.evaluate(() => ({
    render: window.__noonWorkerValidation.renderMessages.find(
      (message) => message?.channel === "noon.render" && message?.type === "ready",
    ),
    engine: window.__noonWorkerValidation.engineMessages.find(
      (message) => message?.channel === "noon.engine" && message?.type === "ready",
    ),
  }));
  diagnostics.ready = ready;
  assert.equal(ready.render.backend, "WebGPU", `expected WebGPU, got ${ready.render.backend}`);
  assert.equal(ready.render.gpuGeneration, 1, "public renderer must expose its GPU generation");

  const captureBefore = await waitForWorkerCapture(renderWorker);
  diagnostics.captureBefore = captureBefore;
  assert.equal(captureBefore.patched, true, `worker WebGPU patch failed: ${captureBefore.patchError}`);
  assert.equal(captureBefore.deviceCount, 1, "expected one public renderer GPUDevice");
  assert.equal(captureBefore.lost[0], null, "public renderer GPUDevice is unexpectedly lost");

  let baseline = null;
  for (let attempt = 0; attempt < 40; attempt += 1) {
    baseline = await requestRenderMetrics(page, 100 + attempt);
    if (baseline.presentedFrames >= 2) break;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  diagnostics.baseline = baseline;
  assert.equal(baseline.backend, "WebGPU");
  assert.equal(baseline.gpuGeneration, 1);
  assert.ok(baseline.presentedFrames >= 2, "public renderer did not establish a healthy baseline");
  assert.ok(baseline.objectCount > 0, "public renderer baseline scene is empty");

  await renderWorker.evaluate(() => {
    const device = globalThis.__noonWebGpuDeviceCapture?.devices?.[0];
    if (!device) throw new Error("captured public renderer GPUDevice is unavailable");
    // WebGPU requires a non-zero usage bitmask. This creates a real uncaptured
    // validation error without intentionally losing or replacing the device.
    device.createBuffer({
      label: "Noon public render-worker validation oracle",
      size: 4,
      usage: 0,
    });
  });

  await page.waitForFunction(
    () =>
      window.__noonWorkerValidation.renderMessages.filter(
        (message) => message?.channel === "noon.render" && message?.type === "recoverable_error",
      ).length >= 1,
    null,
    { timeout: 10_000 },
  );
  await new Promise((resolve) => setTimeout(resolve, 150));

  const reported = await page.evaluate(() => {
    const messages = window.__noonWorkerValidation.renderMessages;
    return {
      recoverable: messages.filter(
        (message) => message?.channel === "noon.render" && message?.type === "recoverable_error",
      ),
      fatal: messages.filter(
        (message) => message?.channel === "noon.render" && message?.type === "error",
      ),
    };
  });
  diagnostics.reported = reported;
  assert.equal(reported.recoverable.length, 1, "validation error must be reported exactly once");
  assert.equal(reported.fatal.length, 0, "validation error must not become a fatal worker error");

  const diagnostic = reported.recoverable[0].diagnostic;
  assert.equal(diagnostic.backend, "WebGPU");
  assert.equal(diagnostic.generation, 1);
  assert.equal(diagnostic.kind, "validation");
  assert.equal(diagnostic.fatal, false);
  assert.match(
    diagnostic.message,
    /WebGPU generation 1 validation error:/,
    `worker diagnostic lacks backend/generation context: ${diagnostic.message}`,
  );

  let continued = null;
  for (let attempt = 0; attempt < 40; attempt += 1) {
    continued = await requestRenderMetrics(page, 200 + attempt);
    if (continued.presentedFrames > baseline.presentedFrames) break;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  diagnostics.continued = continued;
  assert.equal(continued.backend, baseline.backend, "validation changed public renderer backend");
  assert.equal(
    continued.gpuGeneration,
    baseline.gpuGeneration,
    "validation changed public renderer GPU generation",
  );
  assert.equal(continued.objectCount, baseline.objectCount, "validation changed public scene objects");
  assert.ok(
    continued.presentedFrames > baseline.presentedFrames,
    "public renderer did not continue presenting after validation error",
  );

  const captureAfter = await readWorkerWebGpuCapture(renderWorker);
  diagnostics.captureAfter = captureAfter;
  assert.equal(captureAfter.deviceCount, 1, "validation unexpectedly requested a replacement GPUDevice");
  assert.equal(captureAfter.lost[0], null, "validation unexpectedly lost the public GPUDevice");

  await page.locator("#scene").screenshot({ path: path.join(artifactDir, "continued.png") });

  const unexpectedConsoleErrors = consoleErrors.filter(
    (message) => !/(validation|buffer.*usage|usage.*buffer)/i.test(message),
  );
  assert.deepEqual(pageErrors, [], `page errors: ${pageErrors.join("\n")}`);
  assert.deepEqual(
    unexpectedConsoleErrors,
    [],
    `unexpected console errors: ${unexpectedConsoleErrors.join("\n")}`,
  );

  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  await writeDiagnostics(diagnostics);
  console.log("✓ public render-worker WebGPU validation is recoverable and rendering continues");
} catch (error) {
  diagnostics.failure = error instanceof Error
    ? { name: error.name, message: error.message, stack: error.stack }
    : String(error);
  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  diagnostics.serverOutput = serverOutput;
  await writeDiagnostics(diagnostics);
  throw error;
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
