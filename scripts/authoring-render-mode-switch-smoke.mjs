import assert from "node:assert/strict";
import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_AUTHORING_RENDER_SWITCH_PORT ?? "4196");
const baseUrl = `http://127.0.0.1:${port}`;

const contentTypes = new Map([
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".mjs", "text/javascript; charset=utf-8"],
  [".wasm", "application/wasm"],
  [".json", "application/json; charset=utf-8"],
]);

const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url, baseUrl);
    const relative = decodeURIComponent(url.pathname).replace(/^\/+/, "");
    const resolved = path.resolve(repoRoot, relative || "web/execution-worker-smoke.html");
    if (resolved !== repoRoot && !resolved.startsWith(`${repoRoot}${path.sep}`)) {
      response.writeHead(403).end("forbidden");
      return;
    }
    const info = await stat(resolved);
    if (!info.isFile()) {
      response.writeHead(404).end("not found");
      return;
    }
    response.setHeader("Cross-Origin-Opener-Policy", "same-origin");
    response.setHeader("Cross-Origin-Embedder-Policy", "require-corp");
    response.setHeader("Cross-Origin-Resource-Policy", "same-origin");
    response.setHeader("Cache-Control", "no-store");
    response.setHeader(
      "Content-Type",
      contentTypes.get(path.extname(resolved)) ?? "application/octet-stream",
    );
    response.writeHead(200);
    createReadStream(resolved).pipe(response);
  } catch (error) {
    response.writeHead(error?.code === "ENOENT" ? 404 : 500).end(String(error));
  }
});
await new Promise((resolve, reject) => {
  server.once("error", reject);
  server.listen(port, "127.0.0.1", resolve);
});

const browserArgs = [
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

const retainedScene = JSON.stringify({
  channel: "noon.authoring.retained",
  protocol_version: 2,
  objects: [
    {
      object: 4503599627370496,
      order: 1,
      text: {
        source: "*Hello* from _Typst!_",
        backend: { kind: "typst", math: false },
        font_size: 64,
        transform: {
          translation: { x: 0, y: 1.1 },
          scale: { x: 1, y: 1 },
          rotation: 0,
        },
        color: { red: 1, green: 1, blue: 1, alpha: 1 },
        opacity: 1,
      },
    },
    {
      object: 4503599627370497,
      order: 4,
      text: {
        source: "frac(x, 2)",
        backend: { kind: "typst", math: true },
        font_size: 72,
        transform: {
          translation: { x: 0, y: -1 },
          scale: { x: 1, y: 1 },
          rotation: 0,
        },
        color: { red: 1, green: 0.8, blue: 0.2, alpha: 1 },
        opacity: 0.9,
      },
    },
  ],
});

async function runMode(browser, transportMode) {
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") {
      browserErrors.push(`console: ${message.text()}`);
    }
  });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });

  const result = await page.evaluate(async ({ transportMode: mode, retainedDocumentJson }) => {
    const wasm = await import("./pkg/noon_web.js");
    await wasm.default();
    const sceneJson = wasm.demoSceneJson();
    const sceneSpecJson = wasm.canonicalRetainedSceneSpecJson(sceneJson, retainedDocumentJson);
    const originalCanvas = document.querySelector("#scene");
    const devicePixelRatio = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.round(originalCanvas.clientWidth * devicePixelRatio));
    const height = Math.max(1, Math.round(originalCanvas.clientHeight * devicePixelRatio));
    originalCanvas.width = width;
    originalCanvas.height = height;

    const renderWorker = new Worker(new URL("./authoring-render-worker.js", location.href), {
      type: "module",
      name: "noon-authoring-render-owner-smoke",
    });
    let nextRequestId = 0;
    const pending = new Map();
    const errors = [];
    let resolveInitialRender;
    let rejectInitialRender;
    const initialRenderReady = new Promise((resolve, reject) => {
      resolveInitialRender = resolve;
      rejectInitialRender = reject;
    });

    renderWorker.addEventListener("message", (event) => {
      const message = event.data;
      try {
        if (
          !message ||
          message.channel !== "noon.render" ||
          message.protocolVersion !== 1
        ) {
          throw new Error("invalid render worker envelope");
        }
        if (message.type === "ready") {
          resolveInitialRender(message);
          return;
        }
        if (message.type === "recoverable_error") {
          errors.push(`recoverable render: ${message.message}`);
          return;
        }
        if (message.type === "error") {
          const error = new Error(message.message || "authoring render worker failed");
          if (message.requestId === null || message.requestId === undefined) {
            rejectInitialRender(error);
            for (const request of pending.values()) request.reject(error);
            pending.clear();
            return;
          }
          const request = pending.get(message.requestId);
          if (request) {
            pending.delete(message.requestId);
            request.reject(error);
          }
          return;
        }
        if (message.requestId !== null && message.requestId !== undefined) {
          const request = pending.get(message.requestId);
          if (!request) throw new Error(`unknown render request ${message.requestId}`);
          pending.delete(message.requestId);
          request.resolve(message);
        }
      } catch (error) {
        errors.push(`render control: ${error}`);
        rejectInitialRender(error);
      }
    });
    renderWorker.addEventListener("error", (event) => {
      const error = new Error(event.message || "authoring render worker crashed");
      errors.push(`render crash: ${error}`);
      rejectInitialRender(error);
      for (const request of pending.values()) request.reject(error);
      pending.clear();
    });

    function renderRequest(type, payload = {}, transfer = []) {
      const requestId = nextRequestId++;
      const response = new Promise((resolve, reject) => {
        pending.set(requestId, { resolve, reject });
      });
      renderWorker.postMessage(
        {
          channel: "noon.render",
          protocolVersion: 1,
          type,
          requestId,
          ...payload,
        },
        transfer,
      );
      return response;
    }

    function createEngine(engineMode, port, session) {
      const retained = engineMode === "retained";
      const worker = new Worker(
        new URL(
          retained ? "./retained-execution-engine-worker.js" : "./execution-engine-worker.js",
          location.href,
        ),
        {
          type: "module",
          name: retained ? "noon-retained-engine-switch-smoke" : "noon-engine-switch-smoke",
        },
      );
      const ready = new Promise((resolve, reject) => {
        worker.addEventListener("message", (event) => {
          const message = event.data;
          if (
            !message ||
            message.channel !== "noon.engine" ||
            message.protocolVersion !== 1
          ) {
            reject(new Error("invalid engine worker envelope"));
            return;
          }
          if (message.type === "ready") {
            resolve(message);
          } else if (message.type === "error") {
            reject(new Error(message.message || `${engineMode} engine failed`));
          }
        });
        worker.addEventListener("error", (event) => {
          reject(new Error(event.message || `${engineMode} engine crashed`));
        });
      });
      worker.postMessage(
        {
          channel: "noon.engine",
          protocolVersion: 1,
          type: "init",
          port,
          ...(retained ? { sceneSpecJson } : { sceneJson }),
          loopDurationSeconds: 4,
          transportMode: mode,
          sharedSlotCapacity: 1024 * 1024,
          session,
        },
        [port],
      );
      return { worker, ready };
    }

    const firstChannel = new MessageChannel();
    const offscreen = originalCanvas.transferControlToOffscreen();
    renderWorker.postMessage(
      {
        channel: "noon.render",
        protocolVersion: 1,
        type: "init",
        mode: "legacy",
        canvas: offscreen,
        port: firstChannel.port2,
        transportMode: mode,
        width,
        height,
      },
      [offscreen, firstChannel.port2],
    );
    let engine = createEngine("legacy", firstChannel.port1, 1);
    const [legacyEngineReady, legacyRenderReady] = await Promise.all([
      engine.ready,
      initialRenderReady,
    ]);
    await new Promise((resolve) => setTimeout(resolve, 250));
    const legacyMetrics = (await renderRequest("metrics")).metrics;

    engine.worker.terminate();
    const retainedChannel = new MessageChannel();
    const retainedSwitch = renderRequest(
      "switch_engine",
      { mode: "retained", port: retainedChannel.port2, transportMode: mode },
      [retainedChannel.port2],
    );
    engine = createEngine("retained", retainedChannel.port1, 2);
    const [retainedEngineReady, retainedRenderReady] = await Promise.all([
      engine.ready,
      retainedSwitch,
    ]);
    await new Promise((resolve) => setTimeout(resolve, 250));
    const retainedMetrics = (await renderRequest("metrics")).metrics;

    engine.worker.terminate();
    const finalChannel = new MessageChannel();
    const legacySwitch = renderRequest(
      "switch_engine",
      { mode: "legacy", port: finalChannel.port2, transportMode: mode },
      [finalChannel.port2],
    );
    engine = createEngine("legacy", finalChannel.port1, 3);
    const [finalEngineReady, finalRenderReady] = await Promise.all([
      engine.ready,
      legacySwitch,
    ]);
    await new Promise((resolve) => setTimeout(resolve, 250));
    const finalMetrics = (await renderRequest("metrics")).metrics;

    const sameCanvas = document.querySelector("#scene") === originalCanvas;
    engine.worker.terminate();
    renderWorker.postMessage({
      channel: "noon.render",
      protocolVersion: 1,
      type: "stop",
    });

    return {
      crossOriginIsolated: window.crossOriginIsolated,
      hasSharedArrayBuffer: typeof SharedArrayBuffer === "function",
      legacyEngineReady,
      legacyRenderReady,
      retainedEngineReady,
      retainedRenderReady,
      finalEngineReady,
      finalRenderReady,
      legacyMetrics,
      retainedMetrics,
      finalMetrics,
      sameCanvas,
      errors,
    };
  }, { transportMode, retainedDocumentJson: retainedScene });

  assert.equal(result.crossOriginIsolated, true);
  assert.equal(result.hasSharedArrayBuffer, true);
  assert.equal(result.sameCanvas, true, `${transportMode}: mode switch replaced the HTML canvas`);
  assert.equal(result.errors.length, 0, result.errors.join("\n"));

  assert.equal(result.legacyEngineReady.retained, undefined);
  assert.equal(result.legacyRenderReady.mode, "legacy");
  assert.match(result.legacyRenderReady.backend, /WebGPU|WebGL2/);
  assert.equal(result.legacyMetrics.mode, "legacy");
  assert.equal(result.legacyMetrics.objectCount, 4);
  assert.ok(result.legacyMetrics.presentedFrames >= 1);

  assert.equal(result.retainedEngineReady.retained, true);
  assert.equal(result.retainedEngineReady.mixed, true);
  assert.equal(result.retainedRenderReady.type, "mode_switched");
  assert.equal(result.retainedRenderReady.mode, "retained");
  assert.equal(result.retainedRenderReady.retained, true);
  assert.equal(result.retainedRenderReady.mixed, true);
  assert.equal(result.retainedMetrics.mode, "retained");
  assert.equal(result.retainedMetrics.objectCount, 6);
  assert.equal(result.retainedMetrics.resourceBundlePending, false);
  assert.equal(result.retainedMetrics.modeSwitches, 1);
  assert.ok(
    result.retainedMetrics.presentedFrames > result.legacyMetrics.presentedFrames,
    `${transportMode}: retained switch reset or failed to advance presentation history`,
  );

  assert.equal(result.finalEngineReady.retained, undefined);
  assert.equal(result.finalRenderReady.type, "mode_switched");
  assert.equal(result.finalRenderReady.mode, "legacy");
  assert.equal(result.finalMetrics.mode, "legacy");
  assert.equal(result.finalMetrics.objectCount, 4);
  assert.equal(result.finalMetrics.modeSwitches, 2);
  assert.ok(
    result.finalMetrics.presentedFrames > result.retainedMetrics.presentedFrames,
    `${transportMode}: legacy switch reset or failed to advance presentation history`,
  );

  assert.deepEqual(browserErrors, []);
  await page.close();
  console.log(
    `✓ persistent authoring render owner ${transportMode}: ` +
      `${result.legacyRenderReady.backend} legacy→retained→legacy, ` +
      `${result.finalMetrics.presentedFrames} cumulative frames`,
  );
}

let browser = null;
try {
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs,
  });
  await runMode(browser, "transferable");
  await runMode(browser, "shared");
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}