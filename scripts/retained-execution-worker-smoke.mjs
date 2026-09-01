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
const port = Number(process.env.NOON_RETAINED_EXECUTION_WORKER_PORT ?? "4181");
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

const textA = 4503599627370496;
const textB = 4503599627370497;
const retainedScene = JSON.stringify({
  channel: "noon.authoring.retained",
  protocol_version: 2,
  objects: [
    {
      object: textA,
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
      object: textB,
      order: 4,
      text: {
        source: "frac(x, 2)",
        backend: { kind: "typst", math: true },
        font_size: 72,
        transform: {
          translation: { x: 0, y: -1.0 },
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

  const started = await page.evaluate(async ({ mode, retainedDocumentJson }) => {
    const wasm = await import("./pkg/noon_web.js");
    await wasm.default();
    const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
    const canvas = document.querySelector("#scene");
    const errors = [];
    const client = new ExecutionWorkerClient(canvas, {
      onError(error, owner) {
        errors.push(`${owner}: ${error}`);
      },
    });
    window.retainedExecutionSmoke = { client, errors };
    const sceneJson = wasm.demoSceneJson();
    const sceneSpecJson = wasm.canonicalRetainedSceneSpecJson(
      sceneJson,
      retainedDocumentJson,
    );
    const ready = await client.startRetainedCanonical(sceneSpecJson, {
      loopDurationSeconds: 4,
      transportMode: mode,
      sharedSlotCapacity: 1024 * 1024,
    });
    return {
      ready,
      legacyObjectCount: JSON.parse(sceneJson).objects.length,
      canonicalObjectCount: JSON.parse(sceneSpecJson).objects.length,
      crossOriginIsolated: window.crossOriginIsolated,
      hasSharedArrayBuffer: typeof SharedArrayBuffer === "function",
    };
  }, { mode: transportMode, retainedDocumentJson: retainedScene });

  assert.equal(started.crossOriginIsolated, true);
  assert.equal(started.hasSharedArrayBuffer, true);
  assert.equal(started.legacyObjectCount, 4);
  assert.equal(started.canonicalObjectCount, 6);
  assert.equal(started.ready.transportMode, transportMode);
  assert.equal(started.ready.engine.retained, true);
  assert.equal(started.ready.engine.mixed, true);
  assert.equal(started.ready.engine.canonical, true);
  assert.equal(started.ready.render.retained, true);
  assert.equal(started.ready.render.mixed, true);
  assert.match(started.ready.render.backend, /WebGPU|WebGL2/);

  await page.waitForTimeout(450);
  const before = await page.evaluate(() => window.retainedExecutionSmoke.client.metrics());
  assert.equal(before.metrics.ready, true);
  assert.equal(before.metrics.retained, true);
  assert.equal(before.metrics.mixed, true);
  assert.equal(before.metrics.objectCount, 6);
  assert.ok(before.metrics.presentedFrames >= 1, `${transportMode}: mixed retained frame was not presented`);
  assert.ok(before.metrics.drawCalls > 0, `${transportMode}: mixed retained scene produced no draw calls`);
  assert.ok(before.metrics.instancesDrawn > 0, `${transportMode}: mixed retained scene produced no instances`);
  assert.ok(before.metrics.bytesUploaded > 0, `${transportMode}: mixed retained scene uploaded no GPU data`);
  assert.equal(before.engineMetrics.retained, true);
  assert.equal(before.engineMetrics.mixed, true);
  assert.equal(before.engineMetrics.canonical, true);
  assert.equal(before.engineMetrics.resourceBundleTransfers, 1);
  assert.ok(before.engineMetrics.resourceBundleBytes > 0);
  assert.ok(before.engineMetrics.time > 0, `${transportMode}: mixed retained engine playhead did not advance`);

  const state = await page.evaluate(() => window.retainedExecutionSmoke.client.state());
  assert.equal("sceneJson" in state, false);
  assert.equal("retainedDocumentJson" in state, false);
  const sceneSpec = JSON.parse(state.sceneSpecJson);
  assert.equal(sceneSpec.objects.length, 6);
  assert.equal(sceneSpec.objects[1].id, textA);
  assert.equal(sceneSpec.objects[4].id, textB);
  assert.equal(state.nextPatchSequence, "0");

  const retimed = await page.evaluate(() =>
    window.retainedExecutionSmoke.client.setLoopDurationSeconds(0.9),
  );
  assert.equal(retimed.nextPatchSequence, "0");
  assert.equal("sceneSpecJson" in retimed, true);
  await page.waitForTimeout(1050);
  const afterRetime = await page.evaluate(() => window.retainedExecutionSmoke.client.metrics());
  assert.ok(afterRetime.engineMetrics.time >= 0 && afterRetime.engineMetrics.time < 0.9);
  assert.equal(afterRetime.engineMetrics.resourceBundleTransfers, 1);
  assert.equal(afterRetime.engineMetrics.canonical, true);
  assert.equal(afterRetime.metrics.objectCount, 6);

  const reconnected = await page.evaluate(async () => {
    async function bounded(label, operation, timeoutMs = 5_000) {
      let timer = null;
      try {
        return await Promise.race([
          Promise.resolve().then(operation),
          new Promise((_, reject) => {
            timer = setTimeout(
              () => reject(new Error(`retained reconnect timed out during ${label}`)),
              timeoutMs,
            );
          }),
        ]);
      } finally {
        if (timer !== null) {
          clearTimeout(timer);
        }
      }
    }

    const client = window.retainedExecutionSmoke.client;
    const canvas = client.canvas;
    const paused = await bounded("pause", () => client.pause());
    const beforeReconnect = await bounded("pre-reconnect metrics", () => client.metrics());
    const ready = await bounded("engine restart", () => client.restart({ failedOwner: "engine" }));
    await new Promise((resolve) => setTimeout(resolve, 350));
    const afterReconnect = await bounded("post-reconnect metrics", () => client.metrics());
    const state = await bounded("post-reconnect state", () => client.state());
    return {
      ready,
      paused,
      state,
      sameCanvas: client.canvas === canvas,
      presentedBefore: beforeReconnect.metrics.presentedFrames,
      presentedAfter: afterReconnect.metrics.presentedFrames,
      metrics: afterReconnect.metrics,
      engineMetrics: afterReconnect.engineMetrics,
    };
  });
  assert.equal(reconnected.ready.session, 2);
  assert.equal(reconnected.ready.transportMode, transportMode);
  assert.equal(reconnected.ready.engine.canonical, true);
  assert.equal(reconnected.ready.render.retained, true);
  assert.equal(reconnected.ready.render.mixed, true);
  assert.equal(reconnected.paused.playing, false);
  assert.equal(
    reconnected.state.playing,
    false,
    `${transportMode}: retained engine reconnect lost paused mode`,
  );
  assert.equal("sceneJson" in reconnected.state, false);
  assert.equal("retainedDocumentJson" in reconnected.state, false);
  assert.equal(JSON.parse(reconnected.state.sceneSpecJson).objects.length, 6);
  assert.equal(reconnected.sameCanvas, true, `${transportMode}: retained engine reconnect replaced canvas`);
  assert.ok(
    reconnected.presentedAfter >= reconnected.presentedBefore,
    `${transportMode}: retained engine reconnect reset renderer presentation history`,
  );
  assert.equal(reconnected.metrics.ready, true);
  assert.equal(reconnected.metrics.objectCount, 6);
  assert.equal(reconnected.metrics.resourceBundlePending, false);
  assert.equal(reconnected.engineMetrics.canonical, true);
  assert.equal(reconnected.engineMetrics.resourceBundleTransfers, 1);
  assert.ok(reconnected.engineMetrics.resourceBundleBytes > 0);
  assert.ok(
    reconnected.engineMetrics.time >= 0 && reconnected.engineMetrics.time < 0.9,
    `${transportMode}: retained engine reconnect forgot the authored loop duration`,
  );
  await page.evaluate(() => window.retainedExecutionSmoke.client.resume());

  const recovered = await page.evaluate(async () => {
    const client = window.retainedExecutionSmoke.client;
    const canvas = client.canvas;
    const ready = await client.restart({ failedOwner: "render" });
    await new Promise((resolve) => setTimeout(resolve, 250));
    const state = await client.state();
    const metrics = await client.metrics();
    return {
      ready,
      state,
      canvasReplaced: client.canvas !== canvas,
      metrics: metrics.metrics,
      engineMetrics: metrics.engineMetrics,
    };
  });
  assert.equal(recovered.ready.session, 3);
  assert.equal(recovered.ready.engine.canonical, true);
  assert.equal(recovered.canvasReplaced, true);
  assert.equal("sceneJson" in recovered.state, false);
  assert.equal("retainedDocumentJson" in recovered.state, false);
  assert.equal(JSON.parse(recovered.state.sceneSpecJson).objects.length, 6);
  assert.equal(recovered.metrics.objectCount, 6);
  assert.equal(recovered.engineMetrics.canonical, true);
  assert.equal(recovered.engineMetrics.resourceBundleTransfers, 1);

  const clientErrors = await page.evaluate(() => window.retainedExecutionSmoke.errors.slice());
  assert.deepEqual(clientErrors, []);
  assert.deepEqual(browserErrors, []);
  await page.evaluate(() => window.retainedExecutionSmoke.client.terminate());
  await page.close();
  console.log(
    `✓ canonical retained execution ${transportMode}: ${before.metrics.backend}, ` +
      `${reconnected.presentedAfter} frames through the shared execution owner`,
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
