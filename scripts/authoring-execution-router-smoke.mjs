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
const port = Number(process.env.NOON_AUTHORING_EXECUTION_ROUTER_PORT ?? "4182");
const baseUrl = `http://127.0.0.1:${port}`;

const contentTypes = new Map([
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".mjs", "text/javascript; charset=utf-8"],
  [".wasm", "application/wasm"],
  [".json", "application/json; charset=utf-8"],
  [".py", "text/x-python; charset=utf-8"],
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

const geometrySource = `from noon import *
scene = Scene()
scene.add(Circle(radius=0.5), Square(side_length=0.7).shift(RIGHT * 1.5))
result = scene
`;
const mixedSource = `from noon import *
scene = Scene()
scene.add(Circle(radius=0.4), Typst("middle", font_size=56), Square(side_length=0.8))
result = scene
`;
let browser = null;
let timer;
try {
  browser = await chromium.launch({
    channel: "chromium", headless: true,
    args: ["--enable-unsafe-webgpu", "--enable-unsafe-swiftshader", "--use-gpu-in-tests",
      "--ignore-gpu-blocklist", "--use-gl=angle", "--use-angle=swiftshader", "--disable-gpu-sandbox"],
  });
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.stack ?? String(error)));
  page.on("console", (message) => { if (message.type() === "error") errors.push(message.text()); });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });
  const result = await Promise.race([
    page.evaluate(async ({ geometrySource, mixedSource }) => {
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
      const authoring = new PythonAuthoringClient();
      const clients = [];
      function createExecution() {
        const canvas = document.createElement("canvas");
        canvas.width = 640; canvas.height = 360;
        canvas.style.width = "640px"; canvas.style.height = "360px";
        document.body.append(canvas);
        const client = new AuthoringExecutionClient(canvas);
        clients.push(client);
        return client;
      }
      async function author(source = geometrySource) {
        const result = await authoring.run(source, {});
        if (!result.semanticExecution) throw new Error("shared descriptor missing");
        return result.semanticExecution;
      }
      async function start(client) {
        await client.startSemanticExecution(await author(), {
          authoringClient: authoring, initiallyPaused: true, transportMode: "transferable",
        });
        await client.advanceTo(0);
      }
      async function cancel(client, operation) {
        const pending = operation();
        client.terminate();
        let error = null;
        try { await pending; } catch (failure) { error = String(failure); }
        let stateError = null;
        try { await client.state(); } catch (failure) { stateError = String(failure); }
        return { error, stateError, mode: client.mode, backend: client.rendererBackend };
      }
      try {
        const execution = createExecution();
        const originalCanvas = execution.canvas;
        await start(execution);
        const geometry = (await execution.metrics()).metrics;
        const mixed = await author(mixedSource);
        const switching = execution.reconcileSemanticExecution(mixed, { authoringClient: authoring });
        const racingMetrics = execution.metrics();
        await switching;
        await racingMetrics;
        await execution.pause();
        await execution.advanceTo(0);
        const text = (await execution.metrics()).metrics;
        let invalidContextError = null;
        try {
          await execution.reconcileSemanticExecution({ contextId: "unknown-semantic-context" }, { authoringClient: authoring });
        } catch (error) { invalidContextError = String(error); }
        const afterFailure = (await execution.metrics()).metrics;
        await execution.reconcileSemanticExecution(await author(), { authoringClient: authoring });
        await execution.pause();
        await execution.advanceTo(0);
        const rerun = (await execution.metrics()).metrics;
        const sameCanvas = execution.canvas === originalCanvas;
        const seek = await execution.seek(0.75);
        await execution.restart();
        await execution.pause();
        await execution.advanceTo(0.75);
        const recovery = (await execution.metrics()).metrics;
        const recoveryCanvasChanged = execution.canvas !== originalCanvas;
        const mode = execution.mode;
        execution.terminate();

        const cold = createExecution();
        const coldDescriptor = await author();
        const cancelledStart = await cancel(cold, () => cold.startSemanticExecution(coldDescriptor, { authoringClient: authoring }));
        const replacing = createExecution();
        await start(replacing);
        const replacementDescriptor = await author(mixedSource);
        const cancelledRerun = await cancel(replacing, () => replacing.reconcileSemanticExecution(replacementDescriptor, { authoringClient: authoring }));
        const restarting = createExecution();
        await start(restarting);
        const cancelledRestart = await cancel(restarting, () => restarting.restart());
        return {
          counts: [geometry.objectCount, text.objectCount, afterFailure.objectCount, rerun.objectCount, recovery.objectCount],
          instances: [geometry.instancesDrawn, text.instancesDrawn, rerun.instancesDrawn, recovery.instancesDrawn],
          sameCanvas, recoveryCanvasChanged, mode, seekTime: seek.time, invalidContextError,
          cancellations: [cancelledStart, cancelledRerun, cancelledRestart],
        };
      } finally {
        for (const client of clients) client.terminate();
        authoring.terminate();
      }
    }, { geometrySource, mixedSource }),
    new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("shared authoring qualification timed out")), 120_000); }),
  ]);
  assert.deepEqual(result.counts, [2, 3, 3, 2, 2]);
  assert.ok(result.instances.every((count) => count > 0));
  assert.equal(result.sameCanvas, true);
  assert.equal(result.recoveryCanvasChanged, true);
  assert.equal(result.mode, "semantic");
  assert.equal(result.seekTime, 0.75);
  assert.ok(result.invalidContextError);
  for (const cancellation of result.cancellations) {
    assert.match(cancellation.error, /terminated during an asynchronous operation/);
    assert.match(cancellation.stateError, /has not been started/);
    assert.equal(cancellation.mode, null);
    assert.equal(cancellation.backend, "");
  }
  assert.deepEqual(errors, []);
  console.log("shared authoring routing/recovery/cancellation ok", JSON.stringify(result));
} finally {
  clearTimeout(timer);
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
