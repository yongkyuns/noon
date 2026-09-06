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
const port = Number(process.env.NOON_EXECUTION_WORKER_PORT ?? "4178");
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

async function startClient(page, transportMode) {
  return page.evaluate(async (mode) => {
    const wasm = await import("./pkg/noon_web.js");
    await wasm.default();
    const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
    const { createExplicitTransportSceneJson } = await import(
      "../scripts/explicit-transport-scene-fixture.js"
    );
    const canvas = document.querySelector("#scene");
    const errors = [];
    const client = new ExecutionWorkerClient(canvas, {
      onError(error, owner) {
        errors.push(`${owner}: ${error}`);
      },
    });
    window.executionSmoke = { client, errors };
    const ready = await client.start(createExplicitTransportSceneJson(wasm), {
      loopDurationSeconds: 4,
      transportMode: mode,
      sharedSlotCapacity: 1024 * 1024,
    });
    return {
      ready,
      crossOriginIsolated: window.crossOriginIsolated,
      hasSharedArrayBuffer: typeof SharedArrayBuffer === "function",
    };
  }, transportMode);
}

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
  const started = await startClient(page, transportMode);
  assert.equal(started.crossOriginIsolated, true, "worker smoke must be cross-origin isolated");
  assert.equal(started.hasSharedArrayBuffer, true, "isolated worker smoke must expose SharedArrayBuffer");
  assert.equal(started.ready.transportMode, transportMode);
  assert.equal(started.ready.render.transportMode, transportMode);
  assert.match(started.ready.render.backend, /WebGPU|WebGL2/);

  await page.waitForTimeout(450);
  const before = await page.evaluate(() => window.executionSmoke.client.metrics());
  assert.equal(before.metrics.ready, true);
  assert.equal(before.metrics.objectCount, 4);
  assert.ok(before.metrics.presentedFrames >= 2, `${transportMode}: renderer did not keep presenting`);
  assert.ok(before.metrics.time > 0, `${transportMode}: engine playhead did not advance`);

  const patch = JSON.stringify({
    version: 1,
    sequence: 0,
    patches: [
      {
        set_transform: {
          object: 0,
          transform: {
            translation: { x: 3.0, y: 0.0 },
            rotation: 0.0,
            scale: { x: 1.0, y: 1.0 },
          },
        },
      },
    ],
  });
  const patchResult = await page.evaluate(
    (json) => window.executionSmoke.client.applyPatchBatch(json),
    patch,
  );
  assert.equal(patchResult.nextPatchSequence, "1");

  const badPatch = JSON.stringify({ version: 1, sequence: 9, patches: [] });
  const errorMessage = await page.evaluate(async (json) => {
    try {
      await window.executionSmoke.client.applyPatchBatch(json);
      return null;
    } catch (error) {
      return String(error);
    }
  }, badPatch);
  assert.match(errorMessage, /expected patch sequence 1, got 9/);

  const beforeRetime = await page.evaluate(() => window.executionSmoke.client.state());
  const retimed = await page.evaluate(() =>
    window.executionSmoke.client.setLoopDurationSeconds(0.9),
  );
  assert.equal(retimed.nextPatchSequence, "1", `${transportMode}: retime reset patch sequence`);
  assert.ok(
    Math.abs(retimed.time - beforeRetime.time) < 0.15,
    `${transportMode}: retime jumped the current playhead`,
  );
  await page.waitForTimeout(650);
  const afterRetime = await page.evaluate(() => window.executionSmoke.client.state());
  assert.ok(
    afterRetime.time >= 0 && afterRetime.time < 0.9,
    `${transportMode}: authored loop duration did not wrap execution time`,
  );
  assert.equal(afterRetime.nextPatchSequence, "1");

  const beforeStall = await page.evaluate(() => window.executionSmoke.client.metrics());
  await page.evaluate(() => {
    const worker = new Worker(
      URL.createObjectURL(
        new Blob(
          [
            "onmessage = (event) => { const end = performance.now() + event.data; while (performance.now() < end) {} ; postMessage('done'); };",
          ],
          { type: "text/javascript" },
        ),
      ),
    );
    window.executionSmoke.hostStallWorker = worker;
    worker.postMessage(500);
  });
  await page.waitForTimeout(220);
  const duringStall = await page.evaluate(() => window.executionSmoke.client.metrics());
  assert.ok(
    duringStall.metrics.presentedFrames > beforeStall.metrics.presentedFrames,
    `${transportMode}: render worker stopped while an isolated host worker was stalled`,
  );

  const reconnected = await page.evaluate(async () => {
    const client = window.executionSmoke.client;
    const canvas = client.canvas;
    const paused = await client.pause();
    const beforeReconnect = await client.metrics();
    const ready = await client.restart({ failedOwner: "engine" });
    await new Promise((resolve) => setTimeout(resolve, 300));
    const afterReconnect = await client.metrics();
    const state = await client.state();
    return {
      ready,
      paused,
      state,
      sameCanvas: client.canvas === canvas,
      presentedBefore: beforeReconnect.metrics.presentedFrames,
      presentedAfter: afterReconnect.metrics.presentedFrames,
      metrics: afterReconnect.metrics,
    };
  });
  assert.equal(reconnected.ready.session, 2);
  assert.equal(reconnected.ready.transportMode, transportMode);
  assert.equal(reconnected.paused.playing, false);
  assert.equal(reconnected.state.playing, false, `${transportMode}: engine reconnect lost paused mode`);
  assert.equal(reconnected.sameCanvas, true, `${transportMode}: engine reconnect replaced canvas`);
  assert.ok(
    reconnected.presentedAfter >= reconnected.presentedBefore,
    `${transportMode}: engine reconnect reset renderer presentation history`,
  );
  assert.equal(reconnected.metrics.ready, true);
  assert.equal(reconnected.metrics.objectCount, 4);
  assert.ok(
    reconnected.metrics.time >= 0 && reconnected.metrics.time < 0.9,
    `${transportMode}: engine reconnect forgot the authored loop duration`,
  );
  await page.evaluate(() => window.executionSmoke.client.resume());

  const clientErrors = await page.evaluate(() => window.executionSmoke.errors.slice());
  assert.deepEqual(clientErrors, []);
  assert.deepEqual(browserErrors, []);
  await page.evaluate(() => {
    window.executionSmoke.hostStallWorker?.terminate();
    window.executionSmoke.client.terminate();
  });
  await page.close();
  console.log(
    `✓ execution workers ${transportMode}: ${before.metrics.backend}, ` +
      `${reconnected.presentedAfter} frames with canvas-preserving engine reconnect`,
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
