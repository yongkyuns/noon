import assert from "node:assert/strict";
import { createReadStream } from "node:fs";
import { readFile, stat } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { PNG } from "pngjs";
import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_SHARED_AUTHORING_SMOKE_PORT ?? "4191");
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

const pythonSource = `from noon import *

class SharedAuthoringSmoke(Scene):
    def construct(self):
        earlier = Square(0.25)
        circle = Circle(radius=1.0)
        label = Text("Noon", font_size=48).shift(LEFT * 2)
        appended = Square(0.25)
        self.add(circle, label)

        # Static style authoring completes before the live session. The live
        # facade then owns property publication and effective-value queries.
        circle.set_fill(BLUE, opacity=0.4)
        live = self.live_execution()

        # Real Rust rejection must not consume an export identity or force a
        # whole-Python-scene checkpoint. Successful append uses the same path.
        class LocalKeys(dict):
            def values(self):
                raise AssertionError("typed binding scanned every object key")
        def reject_checkpoint(*_args, **_kwargs):
            raise AssertionError("typed binding checkpointed the whole scene")
        self._object_keys = LocalKeys(self._object_keys)
        self._authoring_checkpoint = reject_checkpoint
        next_id = self._next_object_id
        try:
            live.add(earlier)
        except Exception:
            pass
        else:
            raise AssertionError("interleaved live membership unexpectedly succeeded")
        assert self._next_object_id == next_id
        assert earlier._scene is None
        live.add(appended)
        assert appended.id == next_id
        live.remove(appended)

        live.set_translation(circle, 2.0, -1.0)
        live.set_scale(circle, 1.5, 0.5)
        center = live.effective_center(circle)
        assert abs(center.x - 2.0) < 1e-9
        assert abs(center.y + 1.0) < 1e-9
        assert circle.style["stroke_join"] == "miter"
        assert circle.style["stroke_cap"] == "butt"

        # These compatibility views are deliberately corrupt after the typed scene
        # is complete. Semantic finalization must neither inspect nor export them.
        self._objects[:] = [{"poison": object()}]
        def reject_export(*_args, **_kwargs):
            raise AssertionError("semantic execution must not export legacy scene state")
        self.to_document = reject_export
        self.to_scene_spec = reject_export
`;

const persistedSceneSource = `from noon import *
import builtins

scene = Scene()
circle = Circle(radius=1.0)
scene.add(circle)
builtins.__noon_persisted_scene = scene
builtins.__noon_persisted_circle = circle
result = scene
`;

const reusePersistedSceneSource = `from noon import *
import builtins

scene = builtins.__noon_persisted_scene
circle = builtins.__noon_persisted_circle
circle.shift((2.0, -1.0, 0.0))
circle.scale((1.5, 0.5))
circle.set_fill(BLUE, opacity=0.4)
center = circle.get_center()
assert abs(circle.width - 3.0) < 1e-9
assert abs(circle.height - 1.0) < 1e-9
assert abs(center.x - 2.0) < 1e-9
assert abs(center.y + 1.0) < 1e-9
result = scene
`;

const ordinaryExportBoundarySource = `from noon import *

class OrdinaryExportBoundary(Scene):
    def construct(self):
        circle = Circle(radius=0.4)
        self.add(circle)
        self.play(circle.animate.shift((2.0, 0.0, 0.0)), run_time=2.0, rate_func=linear)
        self.wait(1.0)
`;

const asyncExportBoundarySource = `from noon import *
import builtins

builtins._noon_export_boundary_setup_count = 0

class AsyncExportBoundary(Scene):
    def setup(self):
        builtins._noon_export_boundary_setup_count += 1
        return super().setup()

    async def construct(self):
        raise AssertionError("async export must reject before construct")
`;

const exportBoundarySentinelSource = `from noon import *
import builtins

assert builtins._noon_export_boundary_setup_count == 0
result = Scene()
`;

const unsupportedJspiSource = `from noon import *
import builtins
import pyodide.ffi

builtins._noon_unsupported_jspi_original = pyodide.ffi.can_run_sync
pyodide.ffi.can_run_sync = lambda: False

class UnsupportedJspiContinuation(Scene):
    def construct(self):
        circle = Circle(radius=0.4)
        self.add(circle)
        builtins._noon_unsupported_jspi_scene = self
        self.play(circle.animate.shift((2.0, 0.0, 0.0)), run_time=1.0, rate_func=linear)
`;

const restoreUnsupportedJspiSource = `from noon import *
import builtins
import pyodide.ffi

try:
    scene = builtins._noon_unsupported_jspi_scene
    context = scene._canonical_authoring_context
    assert context.liveExecutionOwnership() == "none"
    assert context.authoredDuration() == 0.0
    assert scene.time == 0.0
finally:
    pyodide.ffi.can_run_sync = builtins._noon_unsupported_jspi_original
    del builtins._noon_unsupported_jspi_original
    del builtins._noon_unsupported_jspi_scene

result = Scene()
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

function visiblePixelStats(
  buffer,
  isVisible = (red, green, blue) => blue >= 40 && blue > red + 15 && blue > green + 3,
) {
  const png = PNG.sync.read(buffer);
  let count = 0;
  let minX = png.width;
  let maxX = -1;
  let minY = png.height;
  let maxY = -1;
  let red = 0;
  let green = 0;
  let blue = 0;
  for (let offset = 0; offset < png.data.length; offset += 4) {
    const pixelRed = png.data[offset];
    const pixelGreen = png.data[offset + 1];
    const pixelBlue = png.data[offset + 2];
    const pixel = offset / 4;
    const x = pixel % png.width;
    const y = Math.floor(pixel / png.width);
    if (!isVisible(pixelRed, pixelGreen, pixelBlue, x, y)) continue;
    count += 1;
    minX = Math.min(minX, x);
    maxX = Math.max(maxX, x);
    minY = Math.min(minY, y);
    maxY = Math.max(maxY, y);
    red += pixelRed;
    green += pixelGreen;
    blue += pixelBlue;
  }
  return {
    count,
    width: maxX >= minX ? maxX - minX + 1 : 0,
    height: maxY >= minY ? maxY - minY + 1 : 0,
    centerX: maxX >= minX ? (minX + maxX) / 2 : 0,
    centerY: maxY >= minY ? (minY + maxY) / 2 : 0,
    meanRed: count === 0 ? 0 : red / count,
    meanGreen: count === 0 ? 0 : green / count,
    meanBlue: count === 0 ? 0 : blue / count,
  };
}

function renderedWorldPixel(buffer, worldX, worldY) {
  const png = PNG.sync.read(buffer);
  const pixelsPerUnit = png.height / 8;
  const x = Math.round(png.width / 2 + worldX * pixelsPerUnit);
  const y = Math.round(png.height / 2 - worldY * pixelsPerUnit);
  const offset = (y * png.width + x) * 4;
  return {
    red: png.data[offset],
    green: png.data[offset + 1],
    blue: png.data[offset + 2],
    alpha: png.data[offset + 3],
  };
}

function textPixelStats(buffer) {
  const png = PNG.sync.read(buffer);
  let count = 0;
  let minX = png.width;
  let maxX = -1;
  let minY = png.height;
  let maxY = -1;
  for (let offset = 0; offset < png.data.length; offset += 4) {
    const red = png.data[offset];
    const green = png.data[offset + 1];
    const blue = png.data[offset + 2];
    if (red < 160 || green < 160 || blue < 160) continue;
    if (Math.max(red, green, blue) - Math.min(red, green, blue) > 24) continue;
    const pixel = offset / 4;
    const x = pixel % png.width;
    const y = Math.floor(pixel / png.width);
    count += 1;
    minX = Math.min(minX, x);
    maxX = Math.max(maxX, x);
    minY = Math.min(minY, y);
    maxY = Math.max(maxY, y);
  }
  return {
    count,
    width: maxX >= minX ? maxX - minX + 1 : 0,
    height: maxY >= minY ? maxY - minY + 1 : 0,
    centerX: maxX >= minX ? (minX + maxX) / 2 : 0,
    centerY: maxY >= minY ? (minY + maxY) / 2 : 0,
  };
}

await new Promise((resolve, reject) => {
  server.once("error", reject);
  server.listen(port, "127.0.0.1", resolve);
});

let browser = null;
try {
  browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs });
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });

  await page.evaluate(async () => {
    const NativeWorker = globalThis.Worker;
    const workerUrls = [];
    globalThis.Worker = new Proxy(NativeWorker, {
      construct(Target, args, newTarget) {
        workerUrls.push(String(args[0]));
        return Reflect.construct(Target, args, newTarget);
      },
    });
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const authoring = new PythonAuthoringClient();
    await authoring.ready();
    window.sharedAuthoringSmoke = {
      AuthoringExecutionClient,
      NativeWorker,
      authoring,
      execution: null,
      workerUrls,
    };
  });

  async function runMode(transportMode, index) {
    const result = await page.evaluate(
      async ({ index, pythonSource, transportMode }) => {
        const harness = window.sharedAuthoringSmoke;
        const canvas = index === 0 ? document.querySelector("#scene") : document.createElement("canvas");
        if (index !== 0) {
          canvas.id = `scene-${transportMode}`;
          canvas.width = 640;
          canvas.height = 360;
          canvas.style.width = "640px";
          canvas.style.height = "360px";
          document.body.append(canvas);
        }
        const authoringResult = await harness.authoring.run(pythonSource, {});
        if ("document" in authoringResult || "sceneSpec" in authoringResult) {
          throw new Error("typed semantic result unexpectedly exported legacy scene state");
        }
        const errors = [];
        const execution = new harness.AuthoringExecutionClient(canvas, {
          onError(error, owner) {
            errors.push(`${owner}: ${error}`);
          },
        });
        harness.execution = execution;
        const ready = await execution.startSemanticExecution(authoringResult.semanticExecution, {
          authoringClient: harness.authoring,
          loopDurationSeconds: 2,
          sharedSlotCapacity: 1024 * 1024,
          transportMode,
        });

        async function settledMetrics(minimumPresentedFrames = 1) {
          let latest = null;
          for (let attempt = 0; attempt < 150; attempt += 1) {
            latest = await execution.metrics();
            if (errors.length) throw new Error(errors.join("; "));
            if (
              latest.metrics.objectCount === 2 &&
              latest.metrics.drawCalls > 0 &&
              latest.metrics.presentedFrames >= minimumPresentedFrames
            ) return latest;
            await new Promise((resolve) => setTimeout(resolve, 20));
          }
          throw new Error(`semantic renderer did not settle: ${JSON.stringify(latest)}`);
        }

        const first = await settledMetrics();
        const paused = await execution.pause();
        const sought = await execution.seek(0.75);
        const resumed = await execution.resume();
        await new Promise((resolve) => setTimeout(resolve, 80));
        const afterResume = await execution.state();

        const rerun = await harness.authoring.run(pythonSource, {});
        if (rerun.semanticExecution.contextId === authoringResult.semanticExecution.contextId) {
          throw new Error("semantic rerun reused its previous authoring context token");
        }
        const rebuilt = await execution.reconcileSemanticExecution(rerun.semanticExecution, {
          authoringClient: harness.authoring,
          loopDurationSeconds: 2,
        });
        const second = await settledMetrics(first.metrics.presentedFrames + 1);
        await execution.pause();
        await execution.seek(0.25);

        return {
          canvasId: execution.canvas.id,
          ready,
          rebuilt,
          first: first.metrics,
          second: second.metrics,
          paused,
          sought,
          resumed,
          afterResume,
          mode: execution.mode,
          workerUrls: [...harness.workerUrls],
        };
      },
      { index, pythonSource, transportMode },
    );

    assert.equal(result.ready.transportMode, transportMode);
    assert.equal(result.ready.render.transportMode, transportMode);
    assert.equal(result.ready.render.mode, "retained");
    assert.ok(
      result.rebuilt.ready.session > result.ready.session,
      `${transportMode}: semantic rerun did not advance the execution session`,
    );
    assert.match(result.ready.render.backend, /WebGPU|WebGL2/);
    assert.equal(result.mode, "semantic");
    assert.equal(result.first.objectCount, 2, `${transportMode}: mixed Text/Circle scene expected`);
    assert.ok(result.first.drawCalls > 0, `${transportMode}: initial frame emitted no draw calls`);
    assert.equal(result.second.objectCount, 2, `${transportMode}: rerun changed object count`);
    assert.ok(result.second.drawCalls > 0, `${transportMode}: rerun emitted no draw calls`);
    assert.equal(result.paused.playing, false);
    assert.ok(Math.abs(result.sought.time - 0.75) < 1e-6);
    assert.equal(result.resumed.playing, true);
    assert.equal(result.afterResume.playing, true, `${transportMode}: resume state was not retained`);
    assert.ok(
      result.workerUrls.some((url) => url.includes("python-worker.js")),
      `${transportMode}: Python authoring worker was not created`,
    );
    assert.ok(
      result.workerUrls.some((url) => url.includes("execution-render-worker.js")),
      `${transportMode}: render worker was not created`,
    );
    assert.equal(
      result.workerUrls.some((url) => url.includes("execution-engine-worker.js")),
      false,
      `${transportMode}: semantic path constructed a JSON execution engine`,
    );

    const screenshot = await page.locator(`#${result.canvasId}`).screenshot();
    const pixels = visiblePixelStats(screenshot);
    const textPixels = textPixelStats(screenshot);
    assert.ok(pixels.count > 1_000, `${transportMode}: rendered circle was blank`);
    assert.ok(pixels.width >= 100 && pixels.width <= 175, `${transportMode}: unexpected width ${pixels.width}`);
    assert.ok(pixels.height >= 30 && pixels.height <= 80, `${transportMode}: unexpected height ${pixels.height}`);
    assert.ok(pixels.centerX > 360, `${transportMode}: circle was not shifted right`);
    assert.ok(pixels.centerY > 195, `${transportMode}: circle was not shifted down`);
    assert.ok(
      pixels.meanBlue > pixels.meanRed + 20,
      `${transportMode}: expected blue fill, got mean red=${pixels.meanRed}, blue=${pixels.meanBlue}`,
    );
    assert.ok(textPixels.count > 100, `${transportMode}: native Text was not visible`);
    assert.ok(textPixels.width > 20, `${transportMode}: native Text had no glyph width`);
    assert.ok(textPixels.centerX < 360, `${transportMode}: native Text was not left of the live circle`);

    await page.evaluate(() => {
      window.sharedAuthoringSmoke.execution.terminate();
      window.sharedAuthoringSmoke.execution = null;
    });
    return { backend: result.ready.render.backend, pixels, textPixels };
  }

  const transferable = await runMode("transferable", 0);
  const shared = await runMode("shared", 1);

  // Run the published examples through the same authoring and rendering harness.
  for (const {
    filename,
    objectCount,
    expectedDuration,
    endpointTime,
    expectText = false,
    staticTextColor = null,
    expectedFinalCenter = null,
    expectedFinalColor = null,
    expectedComposition = false,
    expectedCamera = false,
    expectedDifferentRotations = false,
  } of [
    {
      filename: "live_semantic_scene.py",
      objectCount: 3,
      expectedDuration: null,
      endpointTime: null,
    },
    {
      filename: "live_affine_animation.py",
      objectCount: 1,
      expectedDuration: 2.25,
      endpointTime: 2,
    },
    {
      filename: "live_affine_completion.py",
      objectCount: 1,
      expectedDuration: 4.25,
      endpointTime: null,
      expectedFinalCenter: [5, -2],
    },
    {
      filename: "ordinary_affine_play.py",
      objectCount: 1,
      expectedDuration: 4,
      endpointTime: null,
      expectedFinalCenter: [5, -1],
    },
    {
      filename: "manim_parity_create_circle.py",
      objectCount: 1,
      expectedDuration: 1,
      endpointTime: null,
      expectedFinalCenter: [0, 0],
    },
    {
      filename: "manim_example_show_uncreate.py",
      objectCount: 0,
      expectedDuration: 1,
      endpointTime: null,
    },
    {
      filename: "manim_example_typst_text.py",
      objectCount: 1,
      expectedDuration: null,
      endpointTime: null,
      expectText: true,
      staticTextColor: "yellow",
    },
    {
      filename: "manim_example_math_typst.py",
      objectCount: 1,
      expectedDuration: null,
      endpointTime: null,
      expectText: true,
      staticTextColor: "white",
    },
    {
      filename: "manim_example_succession.py",
      objectCount: 4,
      expectedDuration: 4,
      endpointTime: null,
      expectedFinalCenter: [0, 0],
    },
    {
      filename: "manim_parity_square_and_circle.py",
      objectCount: 2,
      expectedDuration: 1,
      endpointTime: null,
      expectedFinalCenter: [1.25, 0],
    },
    {
      filename: "manim_parity_square_to_circle.py",
      objectCount: 0,
      expectedDuration: 3,
      endpointTime: null,
    },
    {
      filename: "manim_parity_animated_square_to_circle.py",
      objectCount: 1,
      expectedDuration: 4,
      endpointTime: null,
      expectedFinalCenter: [0, 0],
    },
    {
      filename: "manim_parity_different_rotations.py",
      objectCount: 2,
      expectedDuration: 3,
      endpointTime: null,
      expectedDifferentRotations: true,
    },
    {
      filename: "manim_example_move_to_target.py",
      objectCount: 1,
      expectedDuration: 1,
      endpointTime: null,
      expectedFinalCenter: [2, 1],
    },
    {
      filename: "manim_example_moving_camera_center.py",
      objectCount: 3,
      expectedDuration: 2.6,
      endpointTime: null,
      expectedCamera: true,
    },
    {
      filename: "ordinary_composition_play.py",
      objectCount: 2,
      expectedDuration: 4,
      endpointTime: null,
      expectedComposition: true,
    },
    {
      filename: "ordinary_style_play.py",
      objectCount: 1,
      expectedDuration: 2,
      endpointTime: null,
      expectedFinalCenter: [0, 0],
      expectedFinalColor: "green",
    },
    {
      filename: "ordinary_paint_play.py",
      objectCount: 1,
      expectedDuration: 2.4,
      endpointTime: null,
      expectedFinalCenter: [0, 0],
      expectedFinalColor: "yellow",
    },
    {
      filename: "live_content_switch.py",
      objectCount: 2,
      expectedDuration: null,
      endpointTime: null,
      expectText: true,
    },
  ]) {
    const source = await readFile(path.join(repoRoot, "web/python/examples", filename), "utf8");
    const result = await page.evaluate(async ({
      source,
      objectCount,
      endpointTime,
      expectText,
      expectedFinalCenter,
      expectedComposition,
      expectedCamera,
      expectedDifferentRotations,
      filename,
    }) => {
      const harness = window.sharedAuthoringSmoke;
      const canvas = document.createElement("canvas");
      canvas.id = `scene-${filename.replaceAll(".", "-")}`;
      canvas.width = 640;
      canvas.height = 360;
      document.body.append(canvas);
      const execution = new harness.AuthoringExecutionClient(canvas);
      let retainForInspection = false;
      try {
        let continuation = null;
        const authored = await harness.authoring.run(source, {}, {
          async onSemanticContinuation(registration) {
            if (continuation !== null) {
              throw new Error(`${filename}: source registered more than one semantic continuation`);
            }
            continuation = registration;
            await execution.startSemanticExecution(registration.semanticExecution, {
              authoringClient: harness.authoring,
              loopDurationSeconds: Math.max(1, registration.duration),
              transportMode: "transferable",
            });
          },
        });
        if (continuation !== null) {
          if (
            authored.semanticExecution.contextId !== continuation.semanticExecution.contextId ||
            authored.semanticExecution.continuationGeneration !== continuation.generation
          ) {
            throw new Error(`${filename}: final source result changed continuation context`);
          }
        } else {
          const options = {
            authoringClient: harness.authoring,
            transportMode: "transferable",
          };
          if (authored.duration > 0) options.loopDurationSeconds = authored.duration;
          if (!authored.semanticExecution) {
            throw new Error(`${filename}: ordinary source did not produce a semantic execution descriptor`);
          }
          await execution.startSemanticExecution(authored.semanticExecution, options);
        }

        async function waitForFrame(afterPresentedFrames = 0) {
          let latest;
          for (let attempt = 0; attempt < 150; attempt += 1) {
            latest = (await execution.metrics()).metrics;
            if (
              latest.objectCount === objectCount &&
              (objectCount === 0 || latest.drawCalls > 0) &&
              latest.presentedFrames > afterPresentedFrames
            ) return latest;
            await new Promise((resolve) => setTimeout(resolve, 20));
          }
          throw new Error(`live example did not render: ${JSON.stringify(latest)}`);
        }

        const initial = await waitForFrame();
        let endpoint = null;
        if (endpointTime !== null) {
          const paused = await execution.pause();
          if (paused.playing) throw new Error("live affine endpoint seek did not pause playback");
          const sought = await execution.seek(endpointTime);
          const rendered = await waitForFrame(initial.presentedFrames);
          endpoint = { time: sought.time, drawCalls: rendered.drawCalls };
        }
        retainForInspection = objectCount === 0 || endpointTime !== null || expectText || expectedFinalCenter !== null || expectedComposition || expectedCamera || expectedDifferentRotations;
        if (retainForInspection) harness.liveExampleExecution = execution;
        return { canvasId: canvas.id, duration: authored.duration, metrics: initial, endpoint };
      } finally {
        if (!retainForInspection) execution.terminate();
      }
    }, { source, objectCount, endpointTime, expectText, expectedFinalCenter, expectedComposition, expectedCamera, expectedDifferentRotations, filename });
    assert.equal(result.metrics.objectCount, objectCount, filename);
    if (objectCount === 0) {
      assert.equal(result.metrics.drawCalls, 0, `${filename}: removed object still draws`);
      const pixels = visiblePixelStats(
        await page.locator(`#${result.canvasId}`).screenshot(),
        (red, green, blue) => Math.max(red, green, blue) > 80,
      );
      assert.equal(pixels.count, 0, `${filename}: removed object remains visible`);
    } else {
      assert.ok(result.metrics.drawCalls > 0, `${filename}: no draw calls`);
    }
    if (expectedDuration !== null) {
      assert.equal(result.duration, expectedDuration, `${filename}: canonical live duration`);
    }
    if (endpointTime !== null) {
      assert.ok(
        Math.abs(result.endpoint.time - endpointTime) < 1e-6,
        `${filename}: endpoint seek`,
      );
      assert.ok(result.endpoint.drawCalls > 0, `${filename}: endpoint produced no draw calls`);
      const endpointPixels = visiblePixelStats(
        await page.locator(`#${result.canvasId}`).screenshot(),
        (red, green, blue) => Math.max(red, green, blue) > 80,
      );
      assert.ok(endpointPixels.count > 100, `${filename}: endpoint circle was not visible`);
      assert.ok(endpointPixels.width > 125, `${filename}: endpoint did not retain scale 2`);
      assert.ok(endpointPixels.height > 125, `${filename}: endpoint did not retain scale 2`);
      assert.ok(endpointPixels.centerX > 420, `${filename}: endpoint did not retain x=4`);
      assert.ok(endpointPixels.centerY > 220, `${filename}: endpoint did not retain y=-2`);
    }
    if (expectedFinalCenter !== null) {
      const finalPixels = visiblePixelStats(
        await page.locator(`#${result.canvasId}`).screenshot(),
        (red, green, blue) => Math.max(red, green, blue) > 80,
      );
      const expectedX = 320 + expectedFinalCenter[0] * 45;
      const expectedY = 180 - expectedFinalCenter[1] * 45;
      assert.ok(finalPixels.count > 100, `${filename}: completed circle was not visible`);
      assert.ok(Math.abs(finalPixels.centerX - expectedX) < 4, `${filename}: completed x endpoint expected ${expectedX}; pixels ${JSON.stringify(finalPixels)}`);
      assert.ok(Math.abs(finalPixels.centerY - expectedY) < 4, `${filename}: completed y endpoint expected ${expectedY}; pixels ${JSON.stringify(finalPixels)}`);
      if (expectedFinalColor === "green") {
        assert.ok(
          finalPixels.meanGreen > finalPixels.meanRed + 30 &&
            finalPixels.meanGreen > finalPixels.meanBlue + 30,
          `${filename}: post-completion green style edit was not rendered: ${JSON.stringify(finalPixels)}`,
        );
      }
      if (expectedFinalColor === "yellow") {
        assert.ok(
          finalPixels.meanRed > finalPixels.meanBlue + 30 &&
            finalPixels.meanGreen > finalPixels.meanBlue + 30,
          `${filename}: post-completion yellow paint edit was not rendered: ${JSON.stringify(finalPixels)}`,
        );
      }
    }
    if (expectedComposition) {
      const screenshot = await page.locator(`#${result.canvasId}`).screenshot();
      const left = renderedWorldPixel(screenshot, -2, 1);
      const right = renderedWorldPixel(screenshot, 2, -1);
      assert.ok(
        left.green > left.red + 80 && left.green > left.blue + 80,
        `${filename}: post-completion left green edit was not rendered: ${JSON.stringify(left)}`,
      );
      assert.ok(
        right.blue > right.red + 80 && right.blue > right.green + 80,
        `${filename}: sequence right blue endpoint was not rendered: ${JSON.stringify(right)}`,
      );
    }
    if (expectedCamera) {
      // Python owns this live source continuation; playback controls are intentionally
      // unavailable. Verify its completed camera view. The paired direct Rust forward
      // proof covers the initial wait, admission, and both movement midpoints/endpoints.
      const cameraView = await page.locator(`#${result.canvasId}`).screenshot();
      const triangle = renderedWorldPixel(cameraView, 0, 0);
      const square = renderedWorldPixel(cameraView, -4, 0);
      assert.ok(
        triangle.green > triangle.red + 25,
        `${filename}: camera did not center the triangle: ${JSON.stringify(triangle)}`,
      );
      assert.ok(
        square.red > square.green + 25,
        `${filename}: camera-relative square position is wrong: ${JSON.stringify(square)}`,
      );
    }
    if (expectedDifferentRotations) {
      // This source owns a live continuation, so inspect only its completed view.
      // The direct Rust proof samples the distinct midpoint interpolation paths.
      const view = await page.locator(`#${result.canvasId}`).screenshot();
      const left = renderedWorldPixel(view, -2, 0);
      const right = renderedWorldPixel(view, 2, 0);
      assert.ok(
        left.blue > left.red + 25 && left.blue > left.green + 10,
        `${filename}: final left square is not blue: ${JSON.stringify(left)}`,
      );
      assert.ok(
        right.green > right.red + 25 && right.green > right.blue + 10,
        `${filename}: final right square is not green: ${JSON.stringify(right)}`,
      );
    }
    if (expectText) {
      const screenshot = await page.locator(`#${result.canvasId}`).screenshot();
      const pixels = staticTextColor === "yellow"
        ? visiblePixelStats(screenshot, (red, green, blue) =>
            red > 180 && green > 140 && red > blue + 40 && green > blue + 30)
        : textPixelStats(screenshot);
      assert.ok(pixels.count > 100, `${filename}: glyphs were not rendered: ${JSON.stringify(pixels)}`);
      assert.ok(pixels.width > 50, `${filename}: text has no glyph extent`);
      if (staticTextColor === null) {
        assert.ok(pixels.centerY < 180, `${filename}: replacement text lost its live position`);
      }
    }
    if (objectCount === 0 || endpointTime !== null || expectText || expectedFinalCenter !== null || expectedComposition || expectedCamera || expectedDifferentRotations) {
      await page.evaluate(() => {
        window.sharedAuthoringSmoke.liveExampleExecution.terminate();
        window.sharedAuthoringSmoke.liveExampleExecution = null;
      });
    }
  }

  // `exportDocument` is the explicit #959 codec boundary. It still runs the
  // shared endpoint authoring operations, but it must never lease a renderer
  // continuation from a normal def construct. Async source cannot satisfy that
  // boundary and rejects before `Scene.setup` mutates the worker-resident scene.
  const exportBoundary = await page.evaluate(async ({
    ordinarySource,
    asyncSource,
    sentinelSource,
  }) => {
    const harness = window.sharedAuthoringSmoke;
    let continuationRegistrations = 0;
    const ordinary = await harness.authoring.run(ordinarySource, {}, {
      exportDocument: true,
      onSemanticContinuation() {
        continuationRegistrations += 1;
        throw new Error("document export must not register a continuation");
      },
    });
    let asyncError = null;
    try {
      await harness.authoring.run(asyncSource, {}, { exportDocument: true });
    } catch (error) {
      asyncError = String(error);
    }
    const sentinel = await harness.authoring.run(sentinelSource, {}, { exportDocument: true });
    return {
      ordinary: {
        duration: ordinary.duration,
        objectCount: ordinary.document.objects.length,
        translation: ordinary.document.objects[0].transform.translation,
        animationTracks: ordinary.document.tracks,
        hasSemanticExecution: Object.hasOwn(ordinary, "semanticExecution"),
      },
      continuationRegistrations,
      asyncError,
      sentinelObjectCount: sentinel.document.objects.length,
    };
  }, {
    ordinarySource: ordinaryExportBoundarySource,
    asyncSource: asyncExportBoundarySource,
    sentinelSource: exportBoundarySentinelSource,
  });
  assert.equal(exportBoundary.ordinary.duration, 3);
  assert.equal(exportBoundary.ordinary.objectCount, 1);
  // Explicit exports retain authored base state and animation tracks rather
  // than baking the live segment endpoint into a static document.
  assert.deepEqual(exportBoundary.ordinary.translation, { x: 0, y: 0 });
  const exportedMotion = exportBoundary.ordinary.animationTracks.find(
    (track) => track.property === "position" || track.property === "transform",
  );
  assert.ok(exportedMotion, JSON.stringify(exportBoundary.ordinary.animationTracks));
  const translation = exportedMotion.values.vec2 ?? {
    from: exportedMotion.values.object.from.transform.translation,
    to: exportedMotion.values.object.to.transform.translation,
  };
  assert.deepEqual(translation, { from: { x: 0, y: 0 }, to: { x: 2, y: 0 } });
  assert.equal(exportedMotion.timing.start_time, 0);
  assert.equal(exportedMotion.timing.duration, 2);
  assert.equal(exportBoundary.ordinary.hasSemanticExecution, false);
  assert.equal(exportBoundary.continuationRegistrations, 0);
  assert.match(
    exportBoundary.asyncError ?? "",
    /exportDocument cannot run an async Scene construct/,
  );
  assert.equal(exportBoundary.sentinelObjectCount, 0);

  // A supported ordinary segment must fail at its JSPI capability gate instead
  // of silently using endpoint-only execution. The restore request verifies the
  // same worker-resident context never activated a player or advanced time.
  const unsupportedJspi = await page.evaluate(async ({ source, restoreSource }) => {
    const harness = window.sharedAuthoringSmoke;
    let error = null;
    try {
      await harness.authoring.run(source, {});
    } catch (failure) {
      error = String(failure);
    } finally {
      await harness.authoring.run(restoreSource, {});
    }
    return { error };
  }, { source: unsupportedJspiSource, restoreSource: restoreUnsupportedJspiSource });
  assert.match(
    unsupportedJspi.error ?? "",
    /ordinary synchronous canonical play\/wait requires Pyodide JS Promise Integration/,
  );

  // Async Python construct suspends on the worker-owned semantic endpoint. The
  // early descriptor starts the existing execution client while runPythonAsync
  // remains unresolved; each endpoint publication returns the same player before
  // the Python continuation authors its next operation.
  const continuationSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_affine_continuation.py"),
    "utf8",
  );
  const continuationResult = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const canvas = document.createElement("canvas");
    canvas.id = "scene-ordinary-affine-continuation";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    let execution = null;
    let registration = null;
    const authored = await harness.authoring.run(source, {}, {
      async onSemanticContinuation(next) {
        if (registration !== null) {
          throw new Error("async source registered more than one semantic context");
        }
        registration = next;
        execution = new harness.AuthoringExecutionClient(canvas);
        await execution.startSemanticExecution(next.semanticExecution, {
          authoringClient: harness.authoring,
          loopDurationSeconds: Math.max(1, next.duration),
          transportMode: "transferable",
        });
      },
    });
    if (execution === null || registration === null) {
      throw new Error("async source did not register its semantic continuation");
    }
    if (authored.semanticExecution.contextId !== registration.semanticExecution.contextId ||
        authored.semanticExecution.continuationGeneration !== registration.generation) {
      throw new Error("final authoring result did not retain its early continuation context");
    }
    let metrics;
    for (let attempt = 0; attempt < 150; attempt += 1) {
      metrics = (await execution.metrics()).metrics;
      if (metrics.objectCount === 1 && metrics.drawCalls > 0 && metrics.presentedFrames > 0) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    harness.liveContinuationExecution = execution;
    return { canvasId: canvas.id, duration: authored.duration, metrics };
  }, continuationSource);
  assert.equal(continuationResult.duration, 4);
  assert.equal(continuationResult.metrics.objectCount, 1);
  assert.ok(continuationResult.metrics.drawCalls > 0);
  const continuationPixels = visiblePixelStats(
    await page.locator(`#${continuationResult.canvasId}`).screenshot(),
    (red, green, blue) => blue > red + 40 && blue > green,
  );
  assert.ok(continuationPixels.count > 100, "async continuation circle was not visible");
  assert.ok(
    Math.abs(continuationPixels.centerX - (320 + 5 * 45)) < 4,
    `async continuation final x was not 5: ${JSON.stringify(continuationPixels)}`,
  );
  assert.ok(
    Math.abs(continuationPixels.centerY - (180 + 45)) < 4,
    `async continuation final y was not -1: ${JSON.stringify(continuationPixels)}`,
  );
  await page.evaluate(() => {
    window.sharedAuthoringSmoke.liveContinuationExecution.terminate();
    window.sharedAuthoringSmoke.liveContinuationExecution = null;
  });

  // Scalar tracker continuation keeps both values and timing in the returned
  // Rust player. Python remains suspended through both tracks and the wait.
  const externalSamples = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const canvas = document.createElement("canvas");
    canvas.id = "scene-external-samples";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    const execution = new harness.AuthoringExecutionClient(canvas);
    harness.externalSampleExecution = execution;
    let resolveAttached;
    let rejectAttached;
    const attached = new Promise((resolve, reject) => {
      resolveAttached = resolve;
      rejectAttached = reject;
    });
    let registrations = 0;
    const authored = harness.authoring.run(source, {}, {
      async onSemanticContinuation(registration) {
        registrations += 1;
        await execution.startSemanticExecution(registration.semanticExecution, {
          authoringClient: harness.authoring,
          transportMode: "transferable",
          pacing: "external_samples",
        });
        resolveAttached();
      },
    });
    authored.catch(rejectAttached);
    await attached;
    try {
      const initial = await execution.state();
      const midpoint = await execution.sampleToAuthoredTime(1);
      // One request crosses the first animation, wait, source edit and next play.
      const crossing = await execution.sampleToAuthoredTime(3.5);
      let rejectedBackwards = false;
      try { await execution.sampleToAuthoredTime(3); }
      catch { rejectedBackwards = true; }
      const afterRejected = await execution.state();
      const endpoint = await execution.sampleToAuthoredTime(4);
      const result = await authored;
      return { initial, midpoint, crossing, endpoint, afterRejected,
        rejectedBackwards, registrations, duration: result.duration, canvasId: canvas.id };
    } catch (error) {
      execution.terminate();
      throw error;
    }
  }, continuationSource);
  assert.equal(externalSamples.initial.time, 0);
  assert.equal(externalSamples.midpoint.time, 1);
  assert.equal(externalSamples.crossing.time, 3.5);
  assert.equal(externalSamples.afterRejected.time, 3.5);
  assert.equal(externalSamples.rejectedBackwards, true);
  assert.equal(externalSamples.endpoint.time, 4);
  assert.equal(externalSamples.duration, 4);
  assert.equal(externalSamples.registrations, 1);
  const externalEndpointPixel = renderedWorldPixel(
    await page.locator(`#${externalSamples.canvasId}`).screenshot(), 5, -1,
  );
  assert.ok(externalEndpointPixel.blue > 180,
    `external sample did not present its exact endpoint: ${JSON.stringify(externalEndpointPixel)}`);

  await page.evaluate(() => window.sharedAuthoringSmoke.externalSampleExecution.terminate());

  const scalarContinuationSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_value_tracker_continuation.py"),
    "utf8",
  );
  const scalarContinuation = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const canvas = document.createElement("canvas");
    canvas.id = "scene-ordinary-value-tracker-continuation";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    let execution = null;
    let registration = null;
    let settled = false;
    let sourceError = null;
    const authoredPromise = harness.authoring.run(source, {}, {
      async onSemanticContinuation(next) {
        if (registration !== null) {
          throw new Error("scalar continuation source registered more than one semantic context");
        }
        registration = next;
        execution = new harness.AuthoringExecutionClient(canvas);
        await execution.startSemanticExecution(next.semanticExecution, {
          authoringClient: harness.authoring,
          loopDurationSeconds: Math.max(1, next.duration),
          transportMode: "transferable",
        });
      },
    });
    authoredPromise.then(() => { settled = true; }, (error) => { sourceError = String(error); });
    for (let attempt = 0; attempt < 150; attempt += 1) {
      if (sourceError !== null) throw new Error(sourceError);
      if (execution !== null && registration !== null) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    if (execution === null || registration === null) {
      throw new Error("scalar continuation source did not register its semantic continuation");
    }
    harness.scalarContinuation = {
      authoredPromise,
      execution,
      registration,
      get settled() { return settled; },
      get sourceError() { return sourceError; },
    };
    return { canvasId: canvas.id };
  }, scalarContinuationSource);

  async function observeScalarDuring(start, end, label) {
    return page.evaluate(async ({ startTime, endTime, phaseLabel }) => {
      const continuation = window.sharedAuthoringSmoke.scalarContinuation;
      let latest = null;
      for (let attempt = 0; attempt < 240; attempt += 1) {
        if (continuation.sourceError !== null) throw new Error(continuation.sourceError);
        if (continuation.settled) break;
        try {
          latest = await continuation.execution.state();
          if (latest.time >= startTime && latest.time < endTime) return latest;
          if (latest.time >= endTime) break;
        } catch {
          // A transferred player is observable again only after the source has
          // authored and returned the next shared segment.
        }
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      throw new Error(`${phaseLabel} did not reach its observable interval: ${JSON.stringify(latest)}`);
    }, { startTime: start, endTime: end, phaseLabel: label });
  }

  const scalarFirstMidpoint = await observeScalarDuring(0.7, 1.3, "scalar first midpoint");
  const scalarFirstPixels = await page.locator(`#${scalarContinuation.canvasId}`).screenshot();
  const scalarFirstX = -2 + scalarFirstMidpoint.time;
  const scalarFirstPixel = renderedWorldPixel(scalarFirstPixels, scalarFirstX, 0);
  assert.ok(
    scalarFirstPixel.red > 180 && scalarFirstPixel.green > 180 && scalarFirstPixel.blue > 180,
    `scalar first track midpoint was not rendered from its captured state: ${JSON.stringify({ scalarFirstMidpoint, scalarFirstPixel })}`,
  );

  const scalarHold = await observeScalarDuring(2.0, 2.85, "scalar persistent hold");
  const scalarHoldPixels = await page.locator(`#${scalarContinuation.canvasId}`).screenshot();
  const scalarHoldPixel = renderedWorldPixel(scalarHoldPixels, 1, 0);
  assert.ok(
    scalarHoldPixel.red > 180 && scalarHoldPixel.green > 180 && scalarHoldPixel.blue > 180,
    `scalar persistent hold did not retain value 3: ${JSON.stringify({ scalarHold, scalarHoldPixel })}`,
  );

  const scalarSecondMidpoint = await observeScalarDuring(3.25, 3.75, "scalar second midpoint");
  const scalarSecondPixels = await page.locator(`#${scalarContinuation.canvasId}`).screenshot();
  const scalarSecondX = 1 + 2 * (scalarSecondMidpoint.time - 3);
  const scalarSecondPixel = renderedWorldPixel(scalarSecondPixels, scalarSecondX, 0);
  assert.ok(
    scalarSecondPixel.red > 180 && scalarSecondPixel.green > 180 && scalarSecondPixel.blue > 180,
    `scalar second track midpoint was not rendered from its captured state: ${JSON.stringify({ scalarSecondMidpoint, scalarSecondPixel })}`,
  );

  const scalarContinuationResult = await page.evaluate(async () => {
    const continuation = window.sharedAuthoringSmoke.scalarContinuation;
    const authored = await continuation.authoredPromise;
    if (
      authored.semanticExecution.contextId !== continuation.registration.semanticExecution.contextId ||
      authored.semanticExecution.continuationGeneration !== continuation.registration.generation
    ) {
      throw new Error("scalar continuation did not retain its early canonical context");
    }
    let metrics;
    for (let attempt = 0; attempt < 150; attempt += 1) {
      metrics = (await continuation.execution.metrics()).metrics;
      if (metrics.objectCount === 1 && metrics.drawCalls > 0 && metrics.presentedFrames > 0) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    window.sharedAuthoringSmoke.scalarContinuationExecution = continuation.execution;
    return { duration: authored.duration, metrics };
  });
  assert.equal(scalarContinuationResult.duration, 4);
  assert.equal(scalarContinuationResult.metrics.objectCount, 1);
  const scalarFinalPixels = await page.locator(`#${scalarContinuation.canvasId}`).screenshot();
  const scalarFinalPixel = renderedWorldPixel(scalarFinalPixels, 3, 0);
  assert.ok(
    scalarFinalPixel.red > 180 && scalarFinalPixel.green > 180 && scalarFinalPixel.blue > 180,
    `scalar continuation did not render its value 5 endpoint: ${JSON.stringify(scalarFinalPixel)}`,
  );
  await page.evaluate(() => {
    window.sharedAuthoringSmoke.scalarContinuationExecution.terminate();
    window.sharedAuthoringSmoke.scalarContinuationExecution = null;
    window.sharedAuthoringSmoke.scalarContinuation = null;
  });

  // Flat composition uses the same source-stack continuation lease. The Rust
  // composition owns child timing; this only attaches/presents its one player.
  const compositionContinuationSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_composition_continuation.py"),
    "utf8",
  );
  const compositionContinuation = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const canvas = document.createElement("canvas");
    canvas.id = "scene-ordinary-composition-continuation";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    let execution = null;
    let registration = null;
    let settled = false;
    const authoredPromise = harness.authoring.run(source, {}, {
      async onSemanticContinuation(next) {
        if (registration !== null) {
          throw new Error("composition source registered more than one semantic context");
        }
        registration = next;
        execution = new harness.AuthoringExecutionClient(canvas);
        await execution.startSemanticExecution(next.semanticExecution, {
          authoringClient: harness.authoring,
          loopDurationSeconds: Math.max(1, next.duration),
          transportMode: "transferable",
        });
      },
    });
    authoredPromise.then(() => { settled = true; }, () => {});
    for (let attempt = 0; attempt < 150; attempt += 1) {
      if (execution !== null && registration !== null) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    if (execution === null || registration === null) {
      throw new Error("composition source did not register its semantic continuation");
    }
    harness.compositionContinuation = { authoredPromise, execution, registration, get settled() { return settled; } };
    return { canvasId: canvas.id };
  }, compositionContinuationSource);

  async function observeCompositionDuring(start, end, label) {
    return page.evaluate(async ({ startTime, endTime, phaseLabel }) => {
      const continuation = window.sharedAuthoringSmoke.compositionContinuation;
      let latest = null;
      for (let attempt = 0; attempt < 200; attempt += 1) {
        if (continuation.settled) break;
        try {
          latest = await continuation.execution.state();
          if (latest.time > startTime && latest.time < endTime) return latest;
          if (latest.time >= endTime) break;
        } catch {
          // The player may be transferred only at an exact endpoint. Keep
          // observing while the source remains suspended on this segment.
        }
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      throw new Error(`${phaseLabel} did not reach its observable interval: ${JSON.stringify(latest)}`);
    }, { startTime: start, endTime: end, phaseLabel: label });
  }

  const compositionParallelMidpoint = await observeCompositionDuring(0.5, 1.5, "parallel midpoint");
  const compositionParallelPixels = await page.locator(
    `#${compositionContinuation.canvasId}`,
  ).screenshot();
  const parallelProgress = compositionParallelMidpoint.time / 2;
  const parallelLeft = renderedWorldPixel(compositionParallelPixels, -2, parallelProgress);
  const parallelRight = renderedWorldPixel(compositionParallelPixels, 2, -parallelProgress);
  assert.ok(
    parallelLeft.red > 180 && parallelLeft.green > 180 && parallelLeft.blue > 180 &&
      parallelRight.red > 180 && parallelRight.green > 180 && parallelRight.blue > 180,
    `composition parallel midpoint was not rendered from its captured state: ${JSON.stringify({ compositionParallelMidpoint, parallelLeft, parallelRight })}`,
  );

  const compositionSequenceMidpoint = await observeCompositionDuring(2.35, 2.55, "sequence midpoint");
  const compositionSequencePixels = await page.locator(
    `#${compositionContinuation.canvasId}`,
  ).screenshot();
  const sequenceLeft = renderedWorldPixel(compositionSequencePixels, -2, 1);
  const sequenceRight = renderedWorldPixel(compositionSequencePixels, 2, -1);
  assert.ok(
    sequenceLeft.red > sequenceLeft.green + 20 && sequenceLeft.red > sequenceLeft.blue + 20 &&
      sequenceRight.red > 180 && sequenceRight.green > 180 && sequenceRight.blue > 180,
    `composition sequence midpoint was not rendered from its captured state: ${JSON.stringify({ compositionSequenceMidpoint, sequenceLeft, sequenceRight })}`,
  );

  const compositionContinuationResult = await page.evaluate(async () => {
    const continuation = window.sharedAuthoringSmoke.compositionContinuation;
    const authored = await continuation.authoredPromise;
    if (
      authored.semanticExecution.contextId !== continuation.registration.semanticExecution.contextId ||
      authored.semanticExecution.continuationGeneration !== continuation.registration.generation
    ) {
      throw new Error("composition continuation did not retain its early canonical context");
    }
    let metrics;
    for (let attempt = 0; attempt < 150; attempt += 1) {
      metrics = (await continuation.execution.metrics()).metrics;
      if (metrics.objectCount === 2 && metrics.drawCalls > 0 && metrics.presentedFrames > 0) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    window.sharedAuthoringSmoke.compositionContinuationExecution = continuation.execution;
    return { duration: authored.duration, metrics };
  });
  assert.equal(compositionContinuationResult.duration, 4);
  assert.equal(compositionContinuationResult.metrics.objectCount, 2);
  const compositionContinuationPixels = await page.locator(
    `#${compositionContinuation.canvasId}`,
  ).screenshot();
  const compositionLeft = renderedWorldPixel(compositionContinuationPixels, -2, 1);
  const compositionRight = renderedWorldPixel(compositionContinuationPixels, 2, -1);
  assert.ok(
    compositionLeft.green > compositionLeft.red + 80 &&
      compositionLeft.green > compositionLeft.blue + 80,
    `composition continuation did not retain its post-segment green edit: ${JSON.stringify(compositionLeft)}`,
  );
  assert.ok(
    compositionRight.blue > compositionRight.red + 80 &&
      compositionRight.blue > compositionRight.green + 80,
    `composition continuation did not retain its sequence endpoint: ${JSON.stringify(compositionRight)}`,
  );
  await page.evaluate(() => {
    window.sharedAuthoringSmoke.compositionContinuationExecution.terminate();
    window.sharedAuthoringSmoke.compositionContinuationExecution = null;
    window.sharedAuthoringSmoke.compositionContinuation = null;
  });

  // A required callback phase is delivered to the already-suspended async
  // source stack. Rust selects the phase and timing; Python returns one exact
  // batch before the endpoint drives the same segment again. The user source
  // remains pending until the completed player is returned.
  const callbackContinuationSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_affine_callback_continuation.py"),
    "utf8",
  );
  const callbackContinuation = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const canvas = document.createElement("canvas");
    canvas.id = "scene-ordinary-affine-callback-continuation";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    let execution = null;
    let registration = null;
    let settled = false;
    let authoringFailure = null;
    const authoredPromise = harness.authoring.run(source, {}, {
      async onSemanticContinuation(next) {
        if (registration !== null) {
          throw new Error("callback continuation source registered more than one semantic context");
        }
        registration = next;
        execution = new harness.AuthoringExecutionClient(canvas);
        harness.callbackContinuationExecution = execution;
        await execution.startSemanticExecution(next.semanticExecution, {
          authoringClient: harness.authoring,
          loopDurationSeconds: Math.max(1, next.duration),
          transportMode: "transferable",
        });
      },
    });
    authoredPromise.then(() => { settled = true; }, (error) => {
      settled = true;
      authoringFailure = String(error?.message ?? error);
    });
    harness.callbackContinuationAuthoredPromise = authoredPromise;

    let midpoint = null;
    for (let attempt = 0; attempt < 150; attempt += 1) {
      if (execution !== null && !settled) {
        try {
          const state = await execution.state();
          if (state.time > 0.15 && state.time < 0.8) {
            midpoint = state;
            break;
          }
        } catch {
          // The player may be returned only after a coherent segment endpoint.
        }
      }
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    if (midpoint === null || settled) {
      throw new Error(authoringFailure ?? "callback continuation source did not remain suspended at a live midpoint");
    }
    return { canvasId: canvas.id, midpoint, registration };
  }, callbackContinuationSource);
  const callbackContinuationMidpointPixels = visiblePixelStats(
    await page.locator(`#${callbackContinuation.canvasId}`).screenshot(),
    (red, green, blue) => blue > red + 20 && blue > green + 10,
  );
  assert.ok(callbackContinuationMidpointPixels.count > 100, "callback continuation midpoint was blank");
  assert.ok(
    callbackContinuationMidpointPixels.centerX > 325 && callbackContinuationMidpointPixels.centerX < 410,
    `callback continuation did not show a live affine midpoint: ${JSON.stringify(callbackContinuationMidpointPixels)}`,
  );
  assert.ok(
    Math.abs(callbackContinuationMidpointPixels.centerY - 135) < 5,
    "ordered callback did not lift the continuation circle",
  );
  const callbackContinuationResult = await page.evaluate(async () => {
    const harness = window.sharedAuthoringSmoke;
    const authored = await harness.callbackContinuationAuthoredPromise;
    const metrics = (await harness.callbackContinuationExecution.metrics()).metrics;
    return { authored, metrics };
  });
  assert.equal(callbackContinuationResult.authored.duration, 1);
  assert.equal(
    callbackContinuationResult.authored.semanticExecution.contextId,
    callbackContinuation.registration.semanticExecution.contextId,
    "callback continuation must retain the early canonical context",
  );
  assert.equal(
    callbackContinuationResult.authored.semanticExecution.continuationGeneration,
    callbackContinuation.registration.generation,
    "callback continuation must retain its one source-run lease generation",
  );
  assert.ok(
    Number.isSafeInteger(callbackContinuationResult.authored.semanticExecution.callbackSessionId),
    "callback continuation must retain the existing host callable session",
  );
  assert.equal(callbackContinuationResult.metrics.objectCount, 1);
  const callbackContinuationFinalPixels = visiblePixelStats(
    await page.locator(`#${callbackContinuation.canvasId}`).screenshot(),
    (red, green, blue) => blue > red + 20 && blue > green + 10,
  );
  assert.ok(callbackContinuationFinalPixels.count > 100, "callback continuation endpoint was blank");
  assert.ok(Math.abs(callbackContinuationFinalPixels.centerX - 410) < 5, "callback continuation endpoint x");
  assert.ok(Math.abs(callbackContinuationFinalPixels.centerY - 135) < 5, "callback continuation ordered lift");
  await page.evaluate(() => {
    window.sharedAuthoringSmoke.callbackContinuationExecution.terminate();
    window.sharedAuthoringSmoke.callbackContinuationExecution = null;
    window.sharedAuthoringSmoke.callbackContinuationAuthoredPromise = null;
  });

  // Sparse callback reads suspend only the active callback invocation. Rust
  // supplies the active circle eagerly, while the Python callback requests its
  // scoped scalar and an inactive anchor object through the exact phase token.
  const callbackSparseReadsSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_callback_sparse_reads.py"),
    "utf8",
  );
  const callbackSparseReads = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const canvas = document.createElement("canvas");
    canvas.id = "scene-ordinary-callback-sparse-reads";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    let execution = null;
    let registration = null;
    let settled = false;
    let authoringFailure = null;
    const authoredPromise = harness.authoring.run(source, {}, {
      async onSemanticContinuation(next) {
        if (registration !== null) {
          throw new Error("sparse callback source registered more than one semantic context");
        }
        registration = next;
        execution = new harness.AuthoringExecutionClient(canvas);
        harness.callbackSparseReadsExecution = execution;
        await execution.startSemanticExecution(next.semanticExecution, {
          authoringClient: harness.authoring,
          loopDurationSeconds: Math.max(1, next.duration),
          transportMode: "transferable",
        });
      },
    });
    authoredPromise.then(() => { settled = true; }, (error) => {
      settled = true;
      authoringFailure = String(error?.message ?? error);
    });
    harness.callbackSparseReadsAuthoredPromise = authoredPromise;

    let trackMidpoint = null;
    for (let attempt = 0; attempt < 180; attempt += 1) {
      if (execution !== null && !settled) {
        try {
          const state = await execution.state();
          // The scalar track begins after the first .25s callback wait.
          if (state.time > 0.55 && state.time < 0.95) {
            trackMidpoint = state;
            break;
          }
        } catch {
          // The endpoint briefly returns the player to the suspended source.
        }
      }
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    if (trackMidpoint === null || settled) {
      throw new Error(authoringFailure ?? "sparse callback source did not remain suspended during its scalar track");
    }
    return { canvasId: canvas.id, trackMidpoint, registration };
  }, callbackSparseReadsSource);
  const callbackSparseTrackPixels = await page.locator(
    `#${callbackSparseReads.canvasId}`,
  ).screenshot();
  const sparseTrackX = -1 + 2 * (callbackSparseReads.trackMidpoint.time - 0.25);
  const sparseTrackPixel = renderedWorldPixel(callbackSparseTrackPixels, sparseTrackX, 1);
  assert.ok(
    sparseTrackPixel.blue > sparseTrackPixel.red + 20 &&
      sparseTrackPixel.blue > sparseTrackPixel.green + 10,
    `sparse scalar callback did not move its active circle from the Rust phase value: ${JSON.stringify({ state: callbackSparseReads.trackMidpoint, sparseTrackPixel })}`,
  );
  const callbackSparseReadsResult = await page.evaluate(async () => {
    const harness = window.sharedAuthoringSmoke;
    const authored = await harness.callbackSparseReadsAuthoredPromise;
    const metrics = (await harness.callbackSparseReadsExecution.metrics()).metrics;
    return { authored, metrics };
  });
  assert.equal(callbackSparseReadsResult.authored.duration, 1.5);
  assert.equal(
    callbackSparseReadsResult.authored.semanticExecution.contextId,
    callbackSparseReads.registration.semanticExecution.contextId,
    "sparse callback continuation must retain its one canonical context",
  );
  assert.equal(
    callbackSparseReadsResult.authored.semanticExecution.continuationGeneration,
    callbackSparseReads.registration.generation,
    "sparse callback continuation must retain its one source-run lease generation",
  );
  assert.ok(
    Number.isSafeInteger(callbackSparseReadsResult.authored.semanticExecution.callbackSessionId),
    "sparse callback continuation must retain its existing host callable session",
  );
  assert.equal(callbackSparseReadsResult.metrics.objectCount, 2);
  const callbackSparseFinalPixels = await page.locator(
    `#${callbackSparseReads.canvasId}`,
  ).screenshot();
  const sparseAnchorPixel = renderedWorldPixel(callbackSparseFinalPixels, -1, 1);
  const sparseCirclePixel = renderedWorldPixel(callbackSparseFinalPixels, 2, 1);
  for (const [label, pixel] of [["anchor", sparseAnchorPixel], ["callback circle", sparseCirclePixel]]) {
    assert.ok(
      pixel.blue > pixel.red + 20 && pixel.blue > pixel.green + 10,
      `sparse callback ${label} was not rendered from the coherent endpoint: ${JSON.stringify(pixel)}`,
    );
  }
  // The Python fixture asserts an accumulating once-per-phase side effect. A
  // successful source result here therefore proves no exact Rust phase token
  // restarted its callback while resolving either sparse read.
  await page.evaluate(() => {
    window.sharedAuthoringSmoke.callbackSparseReadsExecution.terminate();
    window.sharedAuthoringSmoke.callbackSparseReadsExecution = null;
    window.sharedAuthoringSmoke.callbackSparseReadsAuthoredPromise = null;
  });

  // A normal def construct uses the canonical JSPI continuation when its first
  // supported play reaches the shared Rust segment barrier. Its source promise
  // stays pending while the one endpoint owns the player and presents a frame.
  const synchronousContinuationSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_affine_synchronous_continuation.py"),
    "utf8",
  );
  const synchronousContinuationResult = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const canvas = document.createElement("canvas");
    canvas.id = "scene-ordinary-affine-synchronous-continuation";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    let execution = null;
    let registration = null;
    let settled = false;
    const authoredPromise = harness.authoring.run(source, {}, {
      async onSemanticContinuation(next) {
        if (registration !== null) {
          throw new Error("synchronous source registered more than one semantic context");
        }
        registration = next;
        execution = new harness.AuthoringExecutionClient(canvas);
        await execution.startSemanticExecution(next.semanticExecution, {
          authoringClient: harness.authoring,
          loopDurationSeconds: Math.max(1, next.duration),
          transportMode: "transferable",
        });
      },
    });
    authoredPromise.then(() => { settled = true; });

    let progressed = null;
    for (let attempt = 0; attempt < 150; attempt += 1) {
      if (execution !== null && !settled) {
        try {
          const state = await execution.state();
          if (state.time > 0.1 && state.time < 3.9) {
            progressed = state;
            break;
          }
        } catch {
          // The exact player is temporarily returned between segments.
        }
      }
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    if (progressed === null || settled) {
      throw new Error("synchronous JSPI source did not remain pending through a live frame");
    }
    const authored = await authoredPromise;
    if (execution === null || registration === null) {
      throw new Error("synchronous source did not register its semantic continuation");
    }
    if (authored.semanticExecution.contextId !== registration.semanticExecution.contextId ||
        authored.semanticExecution.continuationGeneration !== registration.generation) {
      throw new Error("synchronous final result did not retain its continuation context");
    }
    let metrics;
    for (let attempt = 0; attempt < 150; attempt += 1) {
      metrics = (await execution.metrics()).metrics;
      if (metrics.objectCount === 1 && metrics.drawCalls > 0 && metrics.presentedFrames > 0) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    harness.liveSynchronousContinuationExecution = execution;
    return { canvasId: canvas.id, duration: authored.duration, progressed, metrics };
  }, synchronousContinuationSource);
  assert.equal(synchronousContinuationResult.duration, 4);
  assert.ok(synchronousContinuationResult.progressed.time > 0.1);
  assert.ok(synchronousContinuationResult.progressed.time < 3.9);
  assert.equal(synchronousContinuationResult.metrics.objectCount, 1);
  assert.ok(synchronousContinuationResult.metrics.drawCalls > 0);
  const synchronousContinuationPixels = visiblePixelStats(
    await page.locator(`#${synchronousContinuationResult.canvasId}`).screenshot(),
    (red, green, blue) => blue > red + 40 && blue > green,
  );
  assert.ok(synchronousContinuationPixels.count > 100, "synchronous continuation circle was not visible");
  assert.ok(
    Math.abs(synchronousContinuationPixels.centerX - (320 + 5 * 45)) < 4,
    `synchronous continuation final x was not 5: ${JSON.stringify(synchronousContinuationPixels)}`,
  );
  assert.ok(
    Math.abs(synchronousContinuationPixels.centerY - (180 + 45)) < 4,
    `synchronous continuation final y was not -1: ${JSON.stringify(synchronousContinuationPixels)}`,
  );
  await page.evaluate(() => {
    window.sharedAuthoringSmoke.liveSynchronousContinuationExecution.terminate();
    window.sharedAuthoringSmoke.liveSynchronousContinuationExecution = null;
  });

  // Fade lifecycle uses the same synchronous JSPI continuation lease. Capture
  // real presented midpoints while Python is suspended, then keep the FadeOut
  // endpoint detached for a separate canonical wait before re-adding its exact
  // semantic handle.
  const fadeContinuationSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_fade_synchronous_continuation.py"),
    "utf8",
  );
  const fadeContinuation = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const canvas = document.createElement("canvas");
    canvas.id = "scene-ordinary-fade-synchronous-continuation";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    let execution = null;
    let registration = null;
    const authoredPromise = harness.authoring.run(source, {}, {
      async onSemanticContinuation(next) {
        if (registration !== null) {
          throw new Error("fade source registered more than one semantic context");
        }
        registration = next;
        execution = new harness.AuthoringExecutionClient(canvas);
        await execution.startSemanticExecution(next.semanticExecution, {
          authoringClient: harness.authoring,
          loopDurationSeconds: Math.max(1, next.duration),
          transportMode: "transferable",
        });
      },
    });
    for (let attempt = 0; attempt < 150 && execution === null; attempt += 1) {
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    if (execution === null || registration === null) {
      throw new Error("fade source did not register its semantic continuation");
    }
    harness.ordinaryFadeContinuation = { authoredPromise, execution, registration };
    return { canvasId: canvas.id };
  }, fadeContinuationSource);

  async function observeFadeDuring(start, end, label, expectedObjectCount = null) {
    return page.evaluate(async ({ startTime, endTime, phaseLabel, objectCount }) => {
      const { execution } = window.sharedAuthoringSmoke.ordinaryFadeContinuation;
      let latest = null;
      for (let attempt = 0; attempt < 200; attempt += 1) {
        try {
          const state = await execution.state();
          latest = state;
          if (state.time >= startTime && state.time <= endTime) {
            if (objectCount === null ||
                (await execution.metrics()).metrics.objectCount === objectCount) {
              return state;
            }
          }
        } catch {
          // The exact player is briefly returned between continuation segments.
        }
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      throw new Error(
        `fade ${phaseLabel} did not reach its observable interval: ` +
        JSON.stringify({ latest }),
      );
    }, { startTime: start, endTime: end, phaseLabel: label, objectCount: expectedObjectCount });
  }

  const fadeInMidpoint = await observeFadeDuring(0.3, 0.5, "FadeIn midpoint");
  const fadeInPixel = renderedWorldPixel(
    await page.locator(`#${fadeContinuation.canvasId}`).screenshot(), 0, 0,
  );
  assert.ok(
    fadeInPixel.blue > 50 && fadeInPixel.blue < 210 &&
      fadeInPixel.green > 15 && fadeInPixel.green < 100,
    `FadeIn midpoint did not present partial appearance: ${JSON.stringify({ fadeInMidpoint, fadeInPixel })}`,
  );
  const fadeOutMidpoint = await observeFadeDuring(1.3, 1.5, "FadeOut midpoint");
  const fadeOutPixel = renderedWorldPixel(
    await page.locator(`#${fadeContinuation.canvasId}`).screenshot(), 0, 0,
  );
  assert.ok(
    fadeOutPixel.blue > 50 && fadeOutPixel.blue < 210 &&
      fadeOutPixel.green > 15 && fadeOutPixel.green < 100,
    `FadeOut midpoint did not present partial appearance: ${JSON.stringify({ fadeOutMidpoint, fadeOutPixel })}`,
  );
  // A clean static wait sleeps until its deadline: published authored time may
  // remain exactly 2.0. Observe the committed renderer membership, not a tick
  // that the runtime has no reason to produce.
  const fadeAbsent = await observeFadeDuring(2.0, 2.1, "detached wait", 0);
  const absentPixels = visiblePixelStats(
    await page.locator(`#${fadeContinuation.canvasId}`).screenshot(),
    (red, green, blue) => blue > 35 && blue > red + 20 && blue > green + 10,
  );
  assert.equal(
    absentPixels.count,
    0,
    `FadeOut endpoint remained visible before re-add: ${JSON.stringify({ fadeAbsent, absentPixels })}`,
  );
  const fadeFinal = await page.evaluate(async () => {
    const continuation = window.sharedAuthoringSmoke.ordinaryFadeContinuation;
    const authored = await continuation.authoredPromise;
    if (
      authored.semanticExecution.contextId !== continuation.registration.semanticExecution.contextId ||
      authored.semanticExecution.continuationGeneration !== continuation.registration.generation
    ) {
      throw new Error("fade result did not retain its continuation context");
    }
    let metrics;
    for (let attempt = 0; attempt < 150; attempt += 1) {
      metrics = (await continuation.execution.metrics()).metrics;
      if (metrics.objectCount === 1 && metrics.drawCalls > 0 && metrics.presentedFrames > 0) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    return { duration: authored.duration, metrics };
  });
  assert.equal(fadeFinal.duration, 2.25);
  assert.equal(fadeFinal.metrics.objectCount, 1);
  assert.ok(fadeFinal.metrics.drawCalls > 0);
  const fadeFinalPixel = renderedWorldPixel(
    await page.locator(`#${fadeContinuation.canvasId}`).screenshot(), 0, 0,
  );
  assert.ok(
    fadeFinalPixel.blue > 230 && fadeFinalPixel.green > 85,
    `same-handle re-add did not restore full authored appearance: ${JSON.stringify(fadeFinalPixel)}`,
  );
  await page.evaluate(() => {
    window.sharedAuthoringSmoke.ordinaryFadeContinuation.execution.terminate();
    window.sharedAuthoringSmoke.ordinaryFadeContinuation = null;
  });

  // Native hosts send normalized input occurrences across the genuine worker
  // control port. The Python scene owns no input values or event cursor; the
  // canonical Rust session evaluates the bindings and publishes each frame.
  const nativeSignalsSource = await readFile(
    path.join(repoRoot, "web/python/examples/live_native_signals.py"), "utf8",
  );
  const nativeSignalsResult = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const authored = await harness.authoring.run(source, {});
    const canvas = document.createElement("canvas");
    canvas.id = "scene-live-native-signals";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    const execution = new harness.AuthoringExecutionClient(canvas);
    await execution.startSemanticExecution(authored.semanticExecution, {
      authoringClient: harness.authoring,
      transportMode: "transferable",
    });
    let initial;
    for (let attempt = 0; attempt < 150; attempt += 1) {
      initial = (await execution.metrics()).metrics;
      if (initial.presentedFrames > 0 && initial.objectCount === 0) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    if (!(initial?.presentedFrames > 0) || initial.objectCount !== 0) {
      throw new Error(`native-signal initial frame did not stay hidden: ${JSON.stringify(initial)}`);
    }
    const paused = await execution.pause();
    if (paused.playing) throw new Error("native-signal execution did not pause");
    harness.liveNativeSignalsExecution = execution;
    return { canvasId: canvas.id, metrics: (await execution.metrics()).metrics };
  }, nativeSignalsSource);

  async function waitForNativeSignalFrame(afterPresentedFrames, objectCount) {
    return page.evaluate(async ({ afterPresentedFrames, objectCount }) => {
      const execution = window.sharedAuthoringSmoke.liveNativeSignalsExecution;
      let latest;
      for (let attempt = 0; attempt < 150; attempt += 1) {
        latest = (await execution.metrics()).metrics;
        if (
          latest.presentedFrames > afterPresentedFrames
          && latest.objectCount === objectCount
          && (objectCount === 0 || latest.drawCalls > 0)
        ) return latest;
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      throw new Error(`native-signal input did not render: ${JSON.stringify(latest)}`);
    }, { afterPresentedFrames, objectCount });
  }

  try {
    const initialNativePixels = visiblePixelStats(
      await page.locator(`#${nativeSignalsResult.canvasId}`).screenshot(),
    );
    assert.ok(initialNativePixels.count < 10, "Space=false must keep the native square hidden");

    await page.evaluate(() => window.sharedAuthoringSmoke.liveNativeSignalsExecution.setNativeStateInput(
      { kind: "key", code: "Space" },
      { kind: "bool", value: true },
    ));
    const visibleNativeMetrics = await waitForNativeSignalFrame(
      nativeSignalsResult.metrics.presentedFrames,
      1,
    );
    const visibleNativePixels = visiblePixelStats(
      await page.locator(`#${nativeSignalsResult.canvasId}`).screenshot(),
    );
    assert.ok(visibleNativePixels.count > 900, "Space=true did not reveal the native square");
    assert.ok(Math.abs(visibleNativePixels.centerX - 320) < 4, "revealed native square shifted unexpectedly");
    assert.ok(Math.abs(visibleNativePixels.centerY - 180) < 4, "revealed native square moved unexpectedly");

    await page.evaluate(() => window.sharedAuthoringSmoke.liveNativeSignalsExecution.setNativeStateInput(
      { kind: "pointer_position" },
      { kind: "vec2", x: 1.5, y: -0.5 },
    ));
    const movedNativeMetrics = await waitForNativeSignalFrame(visibleNativeMetrics.presentedFrames, 1);
    const movedNativePixels = visiblePixelStats(
      await page.locator(`#${nativeSignalsResult.canvasId}`).screenshot(),
    );
    assert.ok(movedNativePixels.count > 900, "pointer position hid the native square");
    assert.ok(Math.abs(movedNativePixels.centerX - (320 + 1.5 * 45)) < 4,
      "pointer position did not translate the native square");
    assert.ok(Math.abs(movedNativePixels.centerY - (180 - -0.5 * 45)) < 4,
      "pointer position did not translate the native square vertically");

    await page.evaluate(() => window.sharedAuthoringSmoke.liveNativeSignalsExecution.setNativeStateInput(
      { kind: "control", name: "opacity" },
      { kind: "scalar", value: 0.4 },
    ));
    const dimmedNativeMetrics = await waitForNativeSignalFrame(movedNativeMetrics.presentedFrames, 1);
    const dimmedNativePixels = visiblePixelStats(
      await page.locator(`#${nativeSignalsResult.canvasId}`).screenshot(),
    );
    assert.ok(dimmedNativePixels.count > 100, "native opacity control hid the square");
    assert.ok(dimmedNativePixels.meanBlue < movedNativePixels.meanBlue * 0.7,
      "native opacity control did not dim the square");

    const clickPixels = async () => visiblePixelStats(
      await page.locator(`#${nativeSignalsResult.canvasId}`).screenshot(),
      (red, green, blue, x, y) => (
        blue >= 40
        && blue > red + 15
        && blue > green + 3
        && Math.abs(x - (320 + 0.99 * 45)) <= 2
        && Math.abs(y - (180 - -0.61 * 45)) <= 2
      ),
    );
    await page.evaluate(() => window.sharedAuthoringSmoke.liveNativeSignalsExecution.emitNativeEvent(
      { kind: "pointer_down", button: 0 },
    ));
    const firstClickNativeMetrics = await waitForNativeSignalFrame(dimmedNativeMetrics.presentedFrames, 1);
    const firstClickNativePixels = await clickPixels();
    assert.ok(firstClickNativePixels.count > 10,
      "first ordered primary-pointer event did not rotate the square");

    await page.evaluate(() => window.sharedAuthoringSmoke.liveNativeSignalsExecution.emitNativeEvent(
      { kind: "pointer_down", button: 0 },
    ));
    await waitForNativeSignalFrame(firstClickNativeMetrics.presentedFrames, 1);
    const secondClickNativePixels = await clickPixels();
    assert.ok(secondClickNativePixels.count < 2,
      "second ordered primary-pointer event did not advance the square rotation");
  } finally {
    await page.evaluate(() => {
      window.sharedAuthoringSmoke.liveNativeSignalsExecution?.terminate();
      window.sharedAuthoringSmoke.liveNativeSignalsExecution = null;
    });
  }

  // A legacy wait before the first canonical scalar play must fail in the real
  // authoring worker. The #959 bridge may select one cursor, never merge them.
  const mixedTimingError = await page.evaluate(async () => {
    const source = `from noon import Circle, RIGHT, Scene, linear

scene = Scene()
circle = Circle(radius=0.4)
scene.add(circle)
progress = scene.value_tracker(0.0)
scene.wait(1.0)
scene.play(progress.animate(run_time=2.0, rate_func=linear).set_value(4.0))
result = scene
`;
    try {
      await window.sharedAuthoringSmoke.authoring.run(source, {});
    } catch (error) {
      return String(error);
    }
    throw new Error("mixed legacy/canonical timing unexpectedly authored a scene");
  });
  assert.match(
    mixedTimingError,
    /canonical ValueTracker\.play cannot follow legacy Scene timing/u,
    "real worker must reject a legacy timing prefix before canonical scalar authoring",
  );

  // Opaque callbacks must progress forward through the required Rust barrier.
  // The exact callback publication for the first ordered target is observed
  // through its normal retained renderer submission and presentation.
  const callbackSource = await readFile(
    path.join(repoRoot, "web/python/examples/live_affine_callbacks.py"), "utf8",
  );
  const callbackResult = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const authored = await harness.authoring.run(source, {});
    const canvas = document.createElement("canvas");
    canvas.id = "scene-live-affine-callbacks";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    const execution = new harness.AuthoringExecutionClient(canvas);
    harness.liveExampleExecution = execution;
    await execution.startSemanticExecution(authored.semanticExecution, {
      authoringClient: harness.authoring,
      loopDurationSeconds: 8,
      transportMode: "transferable",
    });
    const paused = await execution.pause();
    const requestedTime = 1.0;
    if (paused.time > requestedTime) {
      throw new Error(
        `callback proof advanced past its deterministic sample before pause: ${paused.time}`,
      );
    }
    const advanced = await execution.advanceToWithRendererObservation(requestedTime);
    if (advanced.time !== requestedTime || advanced.playing !== false) {
      throw new Error(
        `exact callback advance did not remain paused at ${requestedTime}: ${JSON.stringify(advanced)}`,
      );
    }
    if (advanced.rendererObservation?.outcome !== "presented") {
      throw new Error(
        `callback publication did not produce retained renderer evidence: ${JSON.stringify(advanced)}`,
      );
    }
    const metrics = (await execution.metrics()).metrics;
    return {
      canvasId: canvas.id,
      paused,
      requestedTime,
      advanced: {
        time: advanced.time,
        playing: advanced.playing,
      },
      rendererObservation: advanced.rendererObservation,
      metrics,
    };
  }, callbackSource);
  assert.equal(callbackResult.paused.playing, false);
  assert.equal(callbackResult.advanced.playing, false);
  assert.equal(callbackResult.advanced.time, callbackResult.requestedTime);
  assert.equal(callbackResult.metrics.objectCount, 3);
  assert.ok(callbackResult.metrics.drawCalls > 0);
  const rendererObservation = callbackResult.rendererObservation;
  const {
    publication, committed, mirrored, prepared, upload, draw, presentation,
  } = rendererObservation;
  assert.equal(rendererObservation.schema_version, 1);
  assert.equal(rendererObservation.backend, "WebGPU");
  assert.ok(Number.isSafeInteger(publication.session));
  assert.ok(Number.isSafeInteger(publication.sequence));
  assert.equal(committed.time, callbackResult.requestedTime);
  assert.equal(committed.dirty, "updated", JSON.stringify(rendererObservation));
  assert.equal(committed.presence, true);
  assert.deepEqual(committed.transform.translation, { x: 1, y: -2 });
  assert.equal(committed.transform.rotation, 0);
  assert.equal(committed.style.fill.alpha, 1);
  assert.equal(committed.style.opacity, 1);
  assert.equal(mirrored.object, committed.object);
  assert.equal(mirrored.frame_index, committed.frame_index);
  assert.equal(mirrored.time, committed.time);
  assert.deepEqual(mirrored.transform, committed.transform);
  assert.deepEqual(mirrored.style, committed.style);
  assert.equal(mirrored.presence, committed.presence);

  assert.equal(prepared.kind, "text");
  assert.equal(prepared.primitive, null);
  assert.equal(prepared.transform, null);
  assert.equal(prepared.style, null);
  assert.equal(prepared.instance_start, null);
  assert.equal(prepared.instance_end, null);
  assert.ok(prepared.render_item_end > prepared.render_item_start);
  assert.equal(prepared.render_item_count, prepared.glyph_item_count);
  assert.ok(prepared.glyph_item_count > 0);
  assert.equal(prepared.glyph_ranges.length, prepared.glyph_item_count);
  assert.ok(prepared.glyph_ranges.every((range) =>
    ["mask", "color"].includes(range.plane) &&
    range.instance_end > range.instance_start &&
    range.instance_dirty === true));
  assert.equal(prepared.full_rebuilds, 0);

  assert.equal(upload.target_write, null);
  assert.ok(upload.target_text_writes.length > 0);
  assert.ok(upload.target_text_writes.every((write) =>
    ["text_mask", "text_color"].includes(write.buffer) &&
    write.instance_end > write.instance_start &&
    write.byte_length > 0 &&
    write.payload_hash > 0));
  assert.ok(upload.text_bytes_uploaded >= upload.target_text_writes
    .reduce((total, write) => total + write.byte_length, 0));
  assert.equal(upload.buffer_reallocations, 0);
  assert.equal(draw.submission_membership, true);
  assert.ok(draw.geometry_draw_calls > 0);
  assert.ok(draw.geometry_instances_drawn >= 2);
  assert.ok(draw.text_draw_calls > 0);
  assert.ok(draw.text_instances_drawn > 0);
  assert.ok(["success", "suboptimal"].includes(presentation.surface_status));
  assert.ok(presentation.presentation_sequence > 0);
  assert.equal(presentation.submit_called, true);
  assert.equal(presentation.present_called, true);

  // The public observation call resolves only after the matching publication
  // reached the renderer surface. Capture those same pixels without seeking,
  // replaying, or polling another browser frame.
  const callbackScreenshot = await page.locator(`#${callbackResult.canvasId}`).screenshot();
  const callbackPixels = {
    animated: visiblePixelStats(callbackScreenshot,
      (r, g, b, x, y) => x > 300 && y < 220 && Math.min(r, g, b) > 35),
    drift: visiblePixelStats(callbackScreenshot,
      (r, g, b, x, y) => x < 250 && y < 220 && Math.min(r, g, b) > 35),
    label: visiblePixelStats(callbackScreenshot,
      (r, g, b, x, y) => x > 300 && x < 430 && y > 230 && Math.min(r, g, b) > 35),
  };
  assert.ok(callbackPixels.animated.count > 500, "ordered callback circle was blank");
  assert.ok(callbackPixels.drift.count > 100, "accumulating callback circle was blank");
  assert.ok(callbackPixels.label.count > 20, "observed callback text was blank");
  assert.ok(Math.abs(callbackPixels.animated.centerX - 365) < 4, "timeline midpoint x");
  assert.ok(Math.abs(callbackPixels.animated.centerY - 135) < 4, "ordered callback lift y");
  assert.ok(Math.abs(callbackPixels.drift.centerX - 185) < 4, "unowned callback x");
  assert.ok(Math.abs(callbackPixels.drift.centerY - (180 - 45 * callbackResult.advanced.time)) < 4,
    "dt callback did not accumulate coherent forward time");
  assert.ok(callbackPixels.drift.meanRed > callbackPixels.animated.meanRed + 30,
    "second ordered callback did not apply half opacity");
  await page.evaluate(() => {
    window.sharedAuthoringSmoke.liveExampleExecution.terminate();
    window.sharedAuthoringSmoke.liveExampleExecution = null;
  });

  const lineSource = await readFile(
    path.join(repoRoot, "web/python/examples/live_line_match_callback.py"), "utf8",
  );
  const lineResult = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const canvas = document.createElement("canvas");
    canvas.id = "scene-line-match";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    const execution = new harness.AuthoringExecutionClient(canvas);
    harness.lineExecution = execution;
    let resolveAttached, rejectAttached;
    const attached = new Promise((resolve, reject) => {
      resolveAttached = resolve;
      rejectAttached = reject;
    });
    const authored = harness.authoring.run(source, {}, {
      async onSemanticContinuation(registration) {
        await execution.startSemanticExecution(registration.semanticExecution, {
          authoringClient: harness.authoring,
          transportMode: "transferable",
          pacing: "external_samples",
        });
        resolveAttached();
      },
    });
    authored.catch(rejectAttached);
    await attached;
    await execution.sampleToAuthoredTime(0);
    await authored;
    return { canvasId: canvas.id, metrics: (await execution.metrics()).metrics };
  }, lineSource);
  assert.equal(lineResult.metrics.objectCount, 3);
  const linePixel = renderedWorldPixel(
    await page.locator(`#${lineResult.canvasId}`).screenshot(), 1.25, 0,
  );
  assert.ok(linePixel.red > 100 && linePixel.green < 100 && linePixel.blue < 100,
    `ordered Line callback lost its placement or red paint: ${JSON.stringify(linePixel)}`);
  await page.evaluate(() => window.sharedAuthoringSmoke.lineExecution.terminate());

  const paintSource = await readFile(
    path.join(repoRoot, "web/python/examples/live_callback_paint.py"), "utf8",
  );
  const paintResult = await page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const authored = await harness.authoring.run(source, {});
    const canvas = document.createElement("canvas");
    canvas.id = "scene-live-callback-paint";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    const execution = new harness.AuthoringExecutionClient(canvas);
    harness.paintExecution = execution;
    try {
      await execution.startSemanticExecution(authored.semanticExecution, {
        authoringClient: harness.authoring,
        transportMode: "transferable",
        initiallyPaused: true,
      });
      const advanced = await execution.advanceToWithRendererObservation(0.5);
      return { canvasId: canvas.id, observation: advanced.rendererObservation };
    } catch (error) {
      execution.terminate();
      throw error;
    }
  }, paintSource);
  assert.equal(paintResult.observation.outcome, "presented");
  const paintStyle = paintResult.observation.committed.style;
  assert.ok(Math.abs(paintStyle.fill.alpha - 0.4) < 1e-6);
  assert.ok(Math.abs(paintStyle.stroke.alpha - 0.75) < 1e-6);
  assert.equal(paintStyle.opacity, 0.5);
  const paintPixel = renderedWorldPixel(
    await page.locator(`#${paintResult.canvasId}`).screenshot(), 1, 0,
  );
  assert.ok(Math.abs(paintPixel.red - 41) < 12 && paintPixel.blue < 25,
    `shared callback paint did not reach retained rendering: ${JSON.stringify(paintPixel)}`);

  await page.evaluate(() => window.sharedAuthoringSmoke.paintExecution.terminate());

  const persisted = await page.evaluate(
    async ({ persistedSceneSource, reusePersistedSceneSource }) => {
      const harness = window.sharedAuthoringSmoke;
      const firstResult = await harness.authoring.run(persistedSceneSource, {});
      const firstCanvas = document.createElement("canvas");
      firstCanvas.width = 640;
      firstCanvas.height = 360;
      firstCanvas.style.width = "640px";
      firstCanvas.style.height = "360px";
      document.body.append(firstCanvas);
      const firstExecution = new harness.AuthoringExecutionClient(firstCanvas);
      await firstExecution.startSemanticExecution(firstResult.semanticExecution, {
        authoringClient: harness.authoring,
        loopDurationSeconds: 2,
        transportMode: "transferable",
      });
      await firstExecution.pause();
      await firstExecution.seek(0.25);
      const firstMetrics = await firstExecution.metrics();

      // Explicitly retire the token, then stop its endpoint. The Python Scene in
      // builtins remains the owner of the shared WASM context and must stay usable.
      await harness.authoring.releaseSemanticExecution(firstResult.semanticExecution.contextId);
      firstExecution.terminate();
      await new Promise((resolve) => setTimeout(resolve, 50));

      const reusedResult = await harness.authoring.run(reusePersistedSceneSource, {});
      if (reusedResult.semanticExecution.contextId === firstResult.semanticExecution.contextId) {
        throw new Error("persisted Scene reuse did not mint a fresh execution token");
      }
      const reusedCanvas = document.createElement("canvas");
      reusedCanvas.id = "scene-persisted-reuse";
      reusedCanvas.width = 640;
      reusedCanvas.height = 360;
      reusedCanvas.style.width = "640px";
      reusedCanvas.style.height = "360px";
      document.body.append(reusedCanvas);
      const reusedExecution = new harness.AuthoringExecutionClient(reusedCanvas);
      const ready = await reusedExecution.startSemanticExecution(reusedResult.semanticExecution, {
        authoringClient: harness.authoring,
        loopDurationSeconds: 2,
        transportMode: "transferable",
      });
      let metrics = null;
      for (let attempt = 0; attempt < 150; attempt += 1) {
        metrics = await reusedExecution.metrics();
        if (
          metrics.metrics.objectCount === 1 &&
          metrics.metrics.drawCalls > 0 &&
          metrics.metrics.presentedFrames > 0
        ) break;
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      await reusedExecution.pause();
      await reusedExecution.seek(0.5);
      harness.execution = reusedExecution;
      return {
        firstContextId: firstResult.semanticExecution.contextId,
        reusedContextId: reusedResult.semanticExecution.contextId,
        firstMetrics: firstMetrics.metrics,
        metrics: metrics?.metrics ?? null,
        backend: ready.render.backend,
      };
    },
    { persistedSceneSource, reusePersistedSceneSource },
  );
  assert.notEqual(persisted.firstContextId, persisted.reusedContextId);
  assert.equal(persisted.firstMetrics.objectCount, 1);
  assert.equal(persisted.metrics?.objectCount, 1);
  assert.ok(persisted.metrics?.drawCalls > 0);
  const persistedPixels = visiblePixelStats(
    await page.locator("#scene-persisted-reuse").screenshot(),
  );
  assert.ok(persistedPixels.count > 1_000, "persisted Scene reuse rendered a blank frame");
  assert.ok(persistedPixels.centerX > 360, "persisted Scene mutation did not shift right");
  assert.ok(persistedPixels.centerY > 195, "persisted Scene mutation did not shift down");

  const isolation = await page.evaluate(() => ({
    crossOriginIsolated,
    hasSharedArrayBuffer: typeof SharedArrayBuffer === "function",
  }));
  assert.deepEqual(isolation, { crossOriginIsolated: true, hasSharedArrayBuffer: true });
  assert.deepEqual(browserErrors, []);

  await page.evaluate(() => {
    const harness = window.sharedAuthoringSmoke;
    harness.execution?.terminate();
    harness.authoring.terminate();
    globalThis.Worker = harness.NativeWorker;
  });
  // Run the unchanged MovingDots construct through the production raster host.
  // Selection/import are host bootstrap; the source's trackers and callbacks are intact.
  const quickstart = await readFile(path.join(repoRoot, "parity/manim-v0.21/quickstart.py"), "utf8");
  const movingDotsClass = quickstart.match(/class MovingDots\(Scene\):[\s\S]*?(?=\n\nclass )/);
  assert.ok(movingDotsClass, "pinned MovingDots source is missing");
  const rasterPage = await browser.newPage({ viewport: { width: 960, height: 540 } });
  try {
    await rasterPage.goto(`${baseUrl}/web/manim-raster-host.html`, { waitUntil: "load" });
    await rasterPage.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
    const loaded = await rasterPage.evaluate(async (source) => {
      await window.noonHostRaster.ready();
      return window.noonHostRaster.load(source, 4);
    }, `from noon import *\n\n${movingDotsClass[0]}\n`);
    assert.equal(loaded.kind, "semantic_execution");
    assert.equal(loaded.objectCount, 3);
    const frameTimes = Array.from({ length: 91 }, (_, index) => index / 30);
    const rendered = await rasterPage.evaluate((times) =>
      window.noonHostRaster.renderThrough(times.length - 1, times), frameTimes);
    assert.equal(rendered.time, 3);
    assert.equal(rendered.authoredDuration, 3);
    assert.equal(rendered.objectCount, 3);
    assert.equal(rendered.presented, true);
    const pixels = await rasterPage.locator("#scene").screenshot();
    const blueDot = renderedWorldPixel(pixels, 5, 0);
    const redLine = renderedWorldPixel(pixels, (5 + 0.58) / 2, 2);
    assert.ok(blueDot.blue > 80 && blueDot.blue > blueDot.red,
      `MovingDots tracker endpoint missing: ${JSON.stringify(blueDot)}`);
    assert.ok(redLine.red > 100 && redLine.red > redLine.green,
      `MovingDots Line endpoint match missing: ${JSON.stringify(redLine)}`);
  } finally {
    await rasterPage.close();
  }
  console.log(
    `✓ shared authoring semantic execution rendered transferable/${transferable.backend} ` +
      `and shared/${shared.backend}; paired live membership and persisted Scene reuse rendered`,
  );
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
