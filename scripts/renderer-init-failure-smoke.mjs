import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_RENDERER_FAILURE_PORT ?? "4191");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_RENDERER_FAILURE_ARTIFACTS ??
    "browser-smoke-artifacts/renderer-init-failure",
);

const webglBrowserArgs = [
  "--disable-features=WebGPU",
  "--enable-unsafe-swiftshader",
  "--ignore-gpu-blocklist",
  "--use-gl=angle",
  "--use-angle=swiftshader",
  "--disable-gpu-sandbox",
  "--disable-dev-shm-usage",
];
const webgpuBrowserArgs = [
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
];

await mkdir(artifactDir, { recursive: true });

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  {
    cwd: repoRoot,
    stdio: ["ignore", "pipe", "pipe"],
  },
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
  throw new Error(`Renderer failure smoke server did not start: ${lastError}\n${serverOutput}`);
}

async function waitForHarness(page) {
  await page.goto(`${baseUrl}/web/browser-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonSmoke?.state.ready === true, null, {
    timeout: 30_000,
  });
  return page.evaluate(() => window.noonSmoke.metrics());
}

async function loadVisibleAuthoringScene(page) {
  return page.evaluate(async () => {
    const wasm = await import("./pkg/noon_web.js");
    await wasm.default();
    const scene = new wasm.AuthoringSceneCore();
    const circle = scene.add(wasm.authoringCircle(0.75));
    scene.moveTo(circle, 0, 0);
    return window.noonSmoke.loadScene(scene.sceneJson());
  });
}

async function waitForExecutionRendererHarness(page) {
  await page.goto(`${baseUrl}/web/execution-renderer-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonExecutionRendererSmoke?.ready === true, null, {
    timeout: 30_000,
  });
  return page.evaluate(() => ({
    error: window.noonExecutionRendererSmoke.error,
    metrics: window.noonExecutionRendererSmoke.metrics,
  }));
}

function collectErrors(page) {
  const pageErrors = [];
  const consoleErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  return { pageErrors, consoleErrors };
}

async function writeDiagnostics(name, diagnostics) {
  await writeFile(
    path.join(artifactDir, `${name}.json`),
    `${JSON.stringify(diagnostics, null, 2)}\n`,
    "utf8",
  );
}

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: webglBrowserArgs,
  });

  // Scenario 1: WebGPU is unavailable, but the main browser player must fall
  // back to WebGL2 and still present a real frame.
  const fallbackPage = await browser.newPage({ viewport: { width: 1000, height: 600 } });
  const fallbackErrors = collectErrors(fallbackPage);
  const fallbackInitial = await waitForHarness(fallbackPage);
  assert.equal(fallbackInitial.error, null, `WebGL fallback failed: ${fallbackInitial.error}`);
  assert.equal(
    fallbackInitial.rendererBackend,
    "WebGL2",
    `WebGPU-disabled renderer selected ${fallbackInitial.rendererBackend}`,
  );
  const fallbackScene = await loadVisibleAuthoringScene(fallbackPage);
  assert.equal(fallbackScene.objectCount, 1, "WebGL fallback did not load the visible authoring fixture");
  const fallbackFrame = await fallbackPage.evaluate(() => window.noonSmoke.renderAt(0.5));
  assert.equal(fallbackFrame.error, null, "WebGL fallback reported a runtime error");
  assert.equal(fallbackFrame.rendererBackend, "WebGL2");
  assert.equal(fallbackFrame.presented, true, "WebGL fallback did not present a frame");
  assert.ok(fallbackFrame.drawCalls > 0, "WebGL fallback emitted no draw calls");
  assert.deepEqual(fallbackErrors.pageErrors, [], "WebGL fallback emitted page errors");
  assert.deepEqual(fallbackErrors.consoleErrors, [], "WebGL fallback emitted console errors");
  await fallbackPage.screenshot({
    path: path.join(artifactDir, "webgl-fallback.png"),
    fullPage: true,
  });
  await writeDiagnostics("webgl-fallback", {
    browserVersion: browser.version(),
    initial: fallbackInitial,
    frame: fallbackFrame,
    ...fallbackErrors,
  });
  await fallbackPage.close();
  console.log("✓ renderer init: WebGPU unavailable falls back to WebGL2 and presents");

  // Scenario 2: keep WebGPU disabled and deterministically reject browser WebGL
  // context creation. The harness must resolve to one stable initialization error
  // instead of hanging, leaving a blank-but-apparently-ready player, or surfacing
  // an unhandled page exception.
  const unavailablePage = await browser.newPage({ viewport: { width: 1000, height: 600 } });
  const unavailableErrors = collectErrors(unavailablePage);
  await unavailablePage.addInitScript(() => {
    const rejectWebGlContexts = (prototype) => {
      if (!prototype || typeof prototype.getContext !== "function") return;
      const originalGetContext = prototype.getContext;
      Object.defineProperty(prototype, "getContext", {
        configurable: true,
        value(type, ...args) {
          const normalized = String(type).toLowerCase();
          if (
            normalized === "webgl" ||
            normalized === "webgl2" ||
            normalized === "experimental-webgl"
          ) {
            return null;
          }
          return originalGetContext.call(this, type, ...args);
        },
      });
    };
    rejectWebGlContexts(HTMLCanvasElement.prototype);
    if (typeof OffscreenCanvas !== "undefined") {
      rejectWebGlContexts(OffscreenCanvas.prototype);
    }
  });

  const unavailable = await waitForHarness(unavailablePage);
  assert.ok(unavailable.error, "both-unavailable renderer unexpectedly initialized");
  assert.equal(
    unavailable.rendererBackend ?? null,
    null,
    "failed renderer reported a live backend",
  );
  assert.match(
    unavailable.error,
    /(adapter|backend|canvas|context|gpu|surface|webgl)/i,
    `renderer failure lacks backend/context information: ${unavailable.error}`,
  );
  assert.deepEqual(
    unavailableErrors.pageErrors,
    [],
    `handled renderer init failure emitted page errors: ${unavailableErrors.pageErrors.join("\n")}`,
  );
  assert.ok(
    unavailableErrors.consoleErrors.length >= 1,
    "renderer init failure should be reported through the harness console diagnostic",
  );
  assert.ok(
    unavailableErrors.consoleErrors.some((message) =>
      /(adapter|backend|canvas|context|gpu|surface|webgl)/i.test(message),
    ),
    `console diagnostic lacks backend/context information: ${unavailableErrors.consoleErrors.join("\n")}`,
  );
  await unavailablePage.screenshot({
    path: path.join(artifactDir, "both-backends-unavailable.png"),
    fullPage: true,
  });
  await writeDiagnostics("both-backends-unavailable", {
    browserVersion: browser.version(),
    metrics: unavailable,
    ...unavailableErrors,
  });
  await unavailablePage.close();
  console.log("✓ renderer init: both backends unavailable fails clearly without an unhandled exception");

  await browser.close();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: webgpuBrowserArgs,
  });

  // Scenario 3 targets the OffscreenCanvas execution renderer directly. WebGPU
  // remains discoverable and returns a real adapter, but device creation fails;
  // the same renderer instance must then initialize WebGL2 and present its
  // initial execution snapshot.
  const deviceFailurePage = await browser.newPage({ viewport: { width: 1000, height: 600 } });
  const deviceFailureErrors = collectErrors(deviceFailurePage);
  await deviceFailurePage.addInitScript(() => {
    const state = { patched: false, patchError: null, requestDeviceCalls: 0 };
    window.__noonWebGpuInitFailure = state;
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
          Object.defineProperty(adapter, "requestDevice", {
            configurable: true,
            value: async () => {
              state.requestDeviceCalls += 1;
              throw new DOMException(
                "Noon injected WebGPU requestDevice failure",
                "OperationError",
              );
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

  const deviceFailure = await waitForExecutionRendererHarness(deviceFailurePage);
  const injection = await deviceFailurePage.evaluate(() => window.__noonWebGpuInitFailure);
  assert.equal(injection.patched, true, `WebGPU device-failure patch failed: ${injection.patchError}`);
  assert.ok(injection.requestDeviceCalls >= 1, "WebGPU requestDevice failure was not exercised");
  assert.equal(
    deviceFailure.error,
    null,
    `WebGPU device failure did not fall back cleanly: ${deviceFailure.error}`,
  );
  assert.equal(
    deviceFailure.metrics?.rendererBackend,
    "WebGL2",
    `WebGPU device failure selected ${deviceFailure.metrics?.rendererBackend}`,
  );
  assert.equal(
    deviceFailure.metrics?.presented,
    true,
    "device-init fallback did not present a frame",
  );
  assert.ok(
    deviceFailure.metrics?.drawCalls > 0,
    "device-init fallback emitted no draw calls",
  );
  assert.ok(
    deviceFailure.metrics?.objectCount > 0,
    "device-init fallback lost the initial execution snapshot",
  );
  assert.deepEqual(
    deviceFailureErrors.pageErrors,
    [],
    `device-init fallback emitted page errors: ${deviceFailureErrors.pageErrors.join("\n")}`,
  );
  await writeDiagnostics("webgpu-device-failure-fallback", {
    browserVersion: browser.version(),
    injection,
    ...deviceFailure,
    ...deviceFailureErrors,
  });
  await deviceFailurePage.close();
  console.log("✓ execution renderer init: WebGPU device failure falls back to WebGL2 and presents");
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
