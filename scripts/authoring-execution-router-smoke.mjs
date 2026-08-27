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

const mixedSource = `from noon import *

class MixedRouterScene(Scene):
    def construct(self):
        self.add(Circle(radius=0.4))
        self.add(Typst("middle", font_size=56))
        self.add(Square(side_length=0.8))
`;

const legacySource = `from noon import *

class LegacyRouterScene(Scene):
    def construct(self):
        self.add(Circle(radius=0.5))
        self.add(Square(side_length=0.7).shift(RIGHT * 1.5))
`;

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

let browser = null;
try {
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs,
  });
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });

  const result = await page.evaluate(async ({ mixedSource, legacySource }) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const {
      AuthoringExecutionClient,
      AUTHORING_EXECUTION_LEGACY,
      AUTHORING_EXECUTION_RETAINED,
    } = await import("./authoring-execution-client.js");

    const originalCanvas = document.querySelector("#scene");
    const errors = [];
    const authoring = new PythonAuthoringClient();
    const execution = new AuthoringExecutionClient(originalCanvas, {
      onError(error, owner) {
        errors.push(`${owner}: ${error}`);
      },
    });
    const emptySceneJson = '{"version":1,"objects":[],"tracks":[]}';
    const initialReady = await execution.start(emptySceneJson, {
      loopDurationSeconds: 4,
      transportMode: "transferable",
    });
    const initialCanvas = execution.canvas;

    const mixed = await authoring.run(mixedSource, {});
    const inFlightLegacyMetrics = execution.metrics();
    const mixedTransition = execution.reconcileScene(JSON.stringify(mixed.document), {
      retainedDocumentJson: JSON.stringify(mixed.retainedDocument),
      callbacks: mixed.callbacks,
      authoringClient: authoring,
      loopDurationSeconds: mixed.duration > 0 ? mixed.duration : null,
    });
    const mixedTransitionMetrics = execution.metrics();
    const mixedTransitionState = execution.state();
    const [legacyRaceMetrics, mixedResult, mixedRaceMetrics, mixedRaceState] = await Promise.all([
      inFlightLegacyMetrics,
      mixedTransition,
      mixedTransitionMetrics,
      mixedTransitionState,
    ]);
    await new Promise((resolve) => setTimeout(resolve, 350));
    const mixedMetrics = await execution.metrics();
    const mixedCanvas = execution.canvas;

    let callbackError = null;
    try {
      await execution.reconcileScene(JSON.stringify(mixed.document), {
        retainedDocumentJson: JSON.stringify(mixed.retainedDocument),
        callbacks: { session_id: 1, slots: [{}] },
        authoringClient: authoring,
      });
    } catch (error) {
      callbackError = String(error);
    }

    const legacy = await authoring.run(legacySource, {});
    const legacyRetainedObjectCount = legacy.retainedDocument?.objects?.length ?? -1;
    const inFlightRetainedMetrics = execution.metrics();
    const legacyTransition = execution.reconcileScene(JSON.stringify(legacy.document), {
      retainedDocumentJson: JSON.stringify(legacy.retainedDocument),
      callbacks: legacy.callbacks,
      authoringClient: authoring,
      loopDurationSeconds: legacy.duration > 0 ? legacy.duration : null,
    });
    const legacyTransitionMetrics = execution.metrics();
    const legacyTransitionState = execution.state();
    const [retainedRaceMetrics, legacyResult, legacyRaceMetrics, legacyRaceState] = await Promise.all([
      inFlightRetainedMetrics,
      legacyTransition,
      legacyTransitionMetrics,
      legacyTransitionState,
    ]);
    await new Promise((resolve) => setTimeout(resolve, 250));
    const legacyMetrics = await execution.metrics();
    const legacyCanvas = execution.canvas;

    const secondLegacy = await execution.reconcileScene(JSON.stringify(legacy.document), {
      retainedDocumentJson: JSON.stringify(legacy.retainedDocument),
      callbacks: legacy.callbacks,
      authoringClient: authoring,
    });

    const state = await execution.state();
    const summary = {
      initialMode: AUTHORING_EXECUTION_LEGACY,
      retainedMode: AUTHORING_EXECUTION_RETAINED,
      initialReady,
      initialCanvasChanged: initialCanvas !== originalCanvas,
      legacyRaceModeBeforeMixed: legacyRaceMetrics.executionMode,
      mixedMode: mixedResult.mode,
      mixedRebuilt: mixedResult.rebuilt,
      mixedCanvasChanged: mixedCanvas !== initialCanvas,
      mixedRaceMode: mixedRaceMetrics.executionMode,
      mixedRaceRetainedChannel: JSON.parse(mixedRaceState.retainedDocumentJson).channel,
      mixedMetrics,
      callbackError,
      legacyRetainedObjectCount,
      retainedRaceModeBeforeLegacy: retainedRaceMetrics.executionMode,
      legacyMode: legacyResult.mode,
      legacyRebuilt: legacyResult.rebuilt,
      legacyCanvasChanged: legacyCanvas !== mixedCanvas,
      legacyRaceMode: legacyRaceMetrics.executionMode,
      legacyRaceObjectCount: JSON.parse(legacyRaceState.sceneJson).objects.length,
      legacyMetrics,
      secondLegacyMode: secondLegacy.mode,
      secondLegacyRebuilt: secondLegacy.rebuilt,
      sameLegacyCanvas: execution.canvas === legacyCanvas,
      state,
      transportMode: execution.transportMode,
      rendererBackend: execution.rendererBackend,
      clientErrors: errors.slice(),
    };
    execution.terminate();
    authoring.terminate();
    return summary;
  }, { mixedSource, legacySource });

  assert.equal(result.initialCanvasChanged, false);
  assert.equal(result.initialReady.transportMode, "transferable");
  assert.equal(result.transportMode, "transferable");
  assert.ok([result.initialMode, result.retainedMode].includes(result.legacyRaceModeBeforeMixed));
  assert.equal(result.mixedMode, result.retainedMode);
  assert.equal(result.mixedRebuilt, true);
  assert.equal(result.mixedCanvasChanged, true);
  assert.equal(result.mixedRaceMode, result.retainedMode);
  assert.equal(result.mixedRaceRetainedChannel, "noon.authoring.retained");
  assert.equal(result.mixedMetrics.executionMode, result.retainedMode);
  assert.equal(result.mixedMetrics.metrics.ready, true);
  assert.equal(result.mixedMetrics.metrics.objectCount, 3);
  assert.ok(result.mixedMetrics.metrics.presentedFrames >= 1);
  assert.equal(result.mixedMetrics.engineMetrics.resourceBundleTransfers, 1);
  assert.ok(result.mixedMetrics.engineMetrics.resourceBundleBytes > 0);
  assert.equal(result.mixedMetrics.engineMetrics.host.enabled, false);
  assert.match(result.callbackError, /retained authoring with Python host callbacks is not supported yet/);

  assert.equal(result.legacyRetainedObjectCount, 0, "geometry-only authoring should emit an empty sidecar");
  assert.ok([result.retainedMode, result.initialMode].includes(result.retainedRaceModeBeforeLegacy));
  assert.equal(result.legacyMode, result.initialMode);
  assert.equal(result.legacyRebuilt, true);
  assert.equal(result.legacyCanvasChanged, true);
  assert.equal(result.legacyRaceMode, result.initialMode);
  assert.equal(result.legacyRaceObjectCount, 2);
  assert.equal(result.legacyMetrics.executionMode, result.initialMode);
  assert.equal(result.legacyMetrics.metrics.objectCount, 2);
  assert.equal(typeof result.legacyMetrics.engineMetrics.host.enabled, "boolean");
  assert.equal(result.secondLegacyMode, result.initialMode);
  assert.equal(result.secondLegacyRebuilt, false);
  assert.equal(result.sameLegacyCanvas, true);
  assert.equal(JSON.parse(result.state.sceneJson).objects.length, 2);
  assert.match(result.rendererBackend, /WebGPU|WebGL2/);
  assert.deepEqual(result.clientErrors, []);
  assert.deepEqual(browserErrors, []);
  console.log(
    `✓ authoring execution router: legacy → retained → legacy on ${result.rendererBackend}`,
  );
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
