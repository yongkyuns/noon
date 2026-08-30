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
const port = Number(process.env.NOON_AUTHORING_RENDER_PREPARE_PORT ?? "4197");
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

const retainedDocumentJson = JSON.stringify({
  channel: "noon.authoring.retained",
  protocol_version: 2,
  objects: [
    {
      object: 4503599627370496,
      order: 0,
      text: {
        source: "Prepared retained Text",
        backend: {
          kind: "native",
          font_family: "DejaVu Sans Mono",
          line_spacing: -1,
        },
        font_size: 48,
        transform: {
          translation: { x: 0, y: 0 },
          scale: { x: 1, y: 1 },
          rotation: 0,
        },
        color: { red: 1, green: 1, blue: 1, alpha: 1 },
        opacity: 1,
      },
    },
  ],
});
const sceneJson = JSON.stringify({ version: 1, objects: [], tracks: [] });

async function runMode(browser, transportMode) {
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });

  const result = await page.evaluate(
    async ({ transportMode: mode, retainedDocumentJson: retained, sceneJson: scene }) => {
      const canvas = document.querySelector("#scene");
      const devicePixelRatio = window.devicePixelRatio || 1;
      const width = Math.max(1, Math.round(canvas.clientWidth * devicePixelRatio));
      const height = Math.max(1, Math.round(canvas.clientHeight * devicePixelRatio));
      canvas.width = width;
      canvas.height = height;

      const renderWorker = new Worker(new URL("./authoring-render-worker.js", location.href), {
        type: "module",
        name: "noon-prepared-render-smoke",
      });
      let nextRequestId = 0;
      const pending = new Map();
      const errors = [];

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
          if (message.type === "recoverable_error") {
            errors.push(`recoverable render: ${message.message}`);
            return;
          }
          if (message.type === "error") {
            const error = new Error(message.message || "prepared render worker failed");
            const request = pending.get(message.requestId);
            if (request) {
              pending.delete(message.requestId);
              request.reject(error);
            } else {
              errors.push(`render error: ${error}`);
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
        }
      });
      renderWorker.addEventListener("error", (event) => {
        const error = new Error(event.message || "prepared render worker crashed");
        errors.push(`render crash: ${error}`);
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

      const offscreen = canvas.transferControlToOffscreen();
      const prepared = await renderRequest(
        "prepare",
        { canvas: offscreen, transportMode: mode, width, height },
        [offscreen],
      );

      const channel = new MessageChannel();
      const renderStarted = renderRequest(
        "start_engine",
        { mode: "retained", port: channel.port2, transportMode: mode },
        [channel.port2],
      );
      const engineWorker = new Worker(
        new URL("./retained-execution-engine-worker.js", location.href),
        { type: "module", name: "noon-prepared-retained-engine-smoke" },
      );
      const engineReady = new Promise((resolve, reject) => {
        engineWorker.addEventListener("message", (event) => {
          const message = event.data;
          if (
            !message ||
            message.channel !== "noon.engine" ||
            message.protocolVersion !== 1
          ) {
            reject(new Error("invalid retained engine worker envelope"));
            return;
          }
          if (message.type === "ready") resolve(message);
          if (message.type === "error") {
            reject(new Error(message.message || "prepared retained engine failed"));
          }
        });
        engineWorker.addEventListener("error", (event) => {
          reject(new Error(event.message || "prepared retained engine crashed"));
        });
      });
      engineWorker.postMessage(
        {
          channel: "noon.engine",
          protocolVersion: 1,
          type: "init",
          port: channel.port1,
          sceneJson: scene,
          retainedDocumentJson: retained,
          loopDurationSeconds: 4,
          transportMode: mode,
          sharedSlotCapacity: 1024 * 1024,
          session: 1,
        },
        [channel.port1],
      );

      const [engine, render] = await Promise.all([engineReady, renderStarted]);
      await new Promise((resolve) => setTimeout(resolve, 250));
      const metrics = (await renderRequest("metrics")).metrics;

      engineWorker.terminate();
      renderWorker.postMessage({
        channel: "noon.render",
        protocolVersion: 1,
        type: "stop",
      });

      return {
        crossOriginIsolated: window.crossOriginIsolated,
        hasSharedArrayBuffer: typeof SharedArrayBuffer === "function",
        prepared,
        engine,
        render,
        metrics,
        errors,
      };
    },
    { transportMode, retainedDocumentJson, sceneJson },
  );

  assert.equal(result.crossOriginIsolated, true);
  assert.equal(result.hasSharedArrayBuffer, true);
  assert.deepEqual(result.errors, []);
  assert.equal(result.prepared.type, "prepared");
  assert.equal(result.prepared.transportMode, transportMode);
  assert.equal(result.prepared.backend, undefined, "prepare must not create a renderer yet");
  assert.equal(result.engine.retained, true);
  assert.equal(result.engine.mixed, true);
  assert.equal(result.render.type, "engine_started");
  assert.equal(result.render.mode, "retained");
  assert.equal(result.render.retained, true);
  assert.equal(result.render.mixed, true);
  assert.match(result.render.backend, /WebGPU|WebGL2/);
  assert.equal(result.metrics.mode, "retained");
  assert.equal(result.metrics.objectCount, 1);
  assert.equal(result.metrics.resourceBundlePending, false);
  assert.ok(result.metrics.presentedFrames >= 1);

  await page.close();
  console.log(
    `✓ prepared render owner ${transportMode}: retained ${result.render.backend}, ` +
      `${result.metrics.presentedFrames} presented frame(s)`,
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
