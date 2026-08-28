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
    const [preMixedRaceMetrics, mixedResult, mixedRaceMetrics, mixedRaceState] = await Promise.all([
      inFlightLegacyMetrics,
      mixedTransition,
      mixedTransitionMetrics,
      mixedTransitionState,
    ]);
    await new Promise((resolve) => setTimeout(resolve, 350));
    const mixedMetrics = await execution.metrics();
    const mixedCanvas = execution.canvas;

    const mixedPause = await execution.pause();
    const mixedPausedCanvas = execution.canvas;
    const mixedPausedState = await execution.state();
    await new Promise((resolve) => setTimeout(resolve, 160));
    const mixedStillPausedState = await execution.state();
    const mixedSeek = await execution.seek(2.5);
    const mixedSeekState = await execution.state();
    const mixedResume = await execution.resume();
    await new Promise((resolve) => setTimeout(resolve, 160));
    const mixedResumedState = await execution.state();

    const mixedRestartReady = await execution.restart();
    const mixedRestartCanvas = execution.canvas;
    const mixedRestartMetrics = await execution.metrics();
    const mixedRestartState = await execution.state();

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

    const legacyPause = await execution.pause();
    const legacyPausedCanvas = execution.canvas;
    const legacyPausedState = await execution.state();
    await new Promise((resolve) => setTimeout(resolve, 160));
    const legacyStillPausedState = await execution.state();
    const legacySeek = await execution.seek(1.5);
    const legacySeekState = await execution.state();
    const legacyResume = await execution.resume();
    await new Promise((resolve) => setTimeout(resolve, 160));
    const legacyResumedState = await execution.state();

    const secondLegacy = await execution.reconcileScene(JSON.stringify(legacy.document), {
      retainedDocumentJson: JSON.stringify(legacy.retainedDocument),
      callbacks: legacy.callbacks,
      authoringClient: authoring,
    });
    const legacyRestartReady = await execution.restart();
    const legacyRestartCanvas = execution.canvas;
    const legacyRestartMetrics = await execution.metrics();

    const state = await execution.state();
    const summary = {
      initialMode: AUTHORING_EXECUTION_LEGACY,
      retainedMode: AUTHORING_EXECUTION_RETAINED,
      initialReady,
      initialCanvasChanged: initialCanvas !== originalCanvas,
      legacyRaceModeBeforeMixed: preMixedRaceMetrics.executionMode,
      mixedMode: mixedResult.mode,
      mixedRebuilt: mixedResult.rebuilt,
      mixedCanvasChanged: mixedCanvas !== initialCanvas,
      mixedRaceMode: mixedRaceMetrics.executionMode,
      mixedRaceRetainedChannel: JSON.parse(mixedRaceState.retainedDocumentJson).channel,
      mixedMetrics,
      mixedPause,
      mixedPausePreservedCanvas: mixedPausedCanvas === mixedCanvas,
      mixedPausedState,
      mixedStillPausedState,
      mixedSeek,
      mixedSeekState,
      mixedResume,
      mixedResumedState,
      mixedRestartMode: mixedRestartReady.mode,
      mixedRestartCanvasChanged: mixedRestartCanvas !== mixedCanvas,
      mixedRestartObjectCount: mixedRestartMetrics.metrics.objectCount,
      mixedRestartRetainedChannel: JSON.parse(mixedRestartState.retainedDocumentJson).channel,
      callbackError,
      legacyRetainedObjectCount,
      retainedRaceModeBeforeLegacy: retainedRaceMetrics.executionMode,
      legacyMode: legacyResult.mode,
      legacyRebuilt: legacyResult.rebuilt,
      legacyCanvasChanged: legacyCanvas !== mixedRestartCanvas,
      legacyRaceMode: legacyRaceMetrics.executionMode,
      legacyRaceObjectCount: JSON.parse(legacyRaceState.sceneJson).objects.length,
      legacyMetrics,
      legacyPause,
      legacyPausePreservedCanvas: legacyPausedCanvas === legacyCanvas,
      legacyPausedState,
      legacyStillPausedState,
      legacySeek,
      legacySeekState,
      legacyResume,
      legacyResumedState,
      secondLegacyMode: secondLegacy.mode,
      secondLegacyRebuilt: secondLegacy.rebuilt,
      legacyRestartMode: legacyRestartReady.mode,
      legacyRestartCanvasChanged: legacyRestartCanvas !== legacyCanvas,
      legacyRestartObjectCount: legacyRestartMetrics.metrics.objectCount,
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
  assert.equal(result.mixedPause.operation, "pause");
  assert.equal(result.mixedPause.playing, false);
  assert.equal(result.mixedPausePreservedCanvas, true);
  assert.equal(result.mixedPausedState.playing, false);
  assert.equal(result.mixedStillPausedState.playing, false);
  assert.equal(result.mixedStillPausedState.time, result.mixedPausedState.time);
  assert.equal(result.mixedSeek.operation, "seek");
  assert.equal(result.mixedSeek.playing, false);
  assert.equal(result.mixedSeek.time, 2.5);
  assert.equal(result.mixedSeekState.time, 2.5);
  assert.equal(result.mixedSeekState.playing, false);
  assert.equal(result.mixedResume.operation, "resume");
  assert.equal(result.mixedResume.playing, true);
  assert.ok(result.mixedResumedState.time > 2.5);
  assert.equal(result.mixedResumedState.playing, true);
  assert.equal(result.mixedRestartMode, result.retainedMode);
  assert.equal(result.mixedRestartCanvasChanged, true);
  assert.equal(result.mixedRestartObjectCount, 3);
  assert.equal(result.mixedRestartRetainedChannel, "noon.authoring.retained");
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
  assert.equal(result.legacyPause.operation, "pause");
  assert.equal(result.legacyPause.playing, false);
  assert.equal(result.legacyPausePreservedCanvas, true);
  assert.equal(result.legacyPausedState.playing, false);
  assert.equal(result.legacyStillPausedState.playing, false);
  assert.equal(result.legacyStillPausedState.time, result.legacyPausedState.time);
  assert.equal(result.legacySeek.operation, "seek");
  assert.equal(result.legacySeek.playing, false);
  assert.equal(result.legacySeek.time, 1.5);
  assert.equal(result.legacySeekState.time, 1.5);
  assert.equal(result.legacySeekState.playing, false);
  assert.equal(result.legacyResume.operation, "resume");
  assert.equal(result.legacyResume.playing, true);
  assert.ok(result.legacyResumedState.time > 1.5);
  assert.equal(result.legacyResumedState.playing, true);
  assert.equal(result.secondLegacyMode, result.initialMode);
  assert.equal(result.secondLegacyRebuilt, false);
  assert.equal(result.legacyRestartMode, result.initialMode);
  assert.equal(result.legacyRestartCanvasChanged, true);
  assert.equal(result.legacyRestartObjectCount, 2);
  assert.equal(JSON.parse(result.state.sceneJson).objects.length, 2);
  assert.match(result.rendererBackend, /WebGPU|WebGL2/);
  assert.deepEqual(result.clientErrors, []);
  assert.deepEqual(browserErrors, []);
  console.log(
    `✓ authoring execution router: deterministic controls across retained/legacy on ${result.rendererBackend}`,
  );
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}

await import("./playground-race-smoke.mjs");
