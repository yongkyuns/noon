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
    expectedFinalCenter = null,
    expectedFinalColor = null,
    expectedComposition = false,
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
      filename: "live_value_tracker.py",
      objectCount: 1,
      expectedDuration: 2,
      endpointTime: null,
      expectedFinalCenter: [2, 0],
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
      filename,
    }) => {
      const harness = window.sharedAuthoringSmoke;
      const authored = await harness.authoring.run(source, {});
      const canvas = document.createElement("canvas");
      canvas.id = `scene-${filename.replaceAll(".", "-")}`;
      canvas.width = 640;
      canvas.height = 360;
      document.body.append(canvas);
      const execution = new harness.AuthoringExecutionClient(canvas);
      let retainForInspection = false;
      try {
        const options = {
          authoringClient: harness.authoring,
          transportMode: "transferable",
        };
        if (authored.duration > 0) options.loopDurationSeconds = authored.duration;
        await execution.startSemanticExecution(authored.semanticExecution, options);

        async function waitForFrame(afterPresentedFrames = 0) {
          let latest;
          for (let attempt = 0; attempt < 150; attempt += 1) {
            latest = (await execution.metrics()).metrics;
            if (
              latest.objectCount === objectCount &&
              latest.drawCalls > 0 &&
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
        retainForInspection = endpointTime !== null || expectText || expectedFinalCenter !== null || expectedComposition;
        if (retainForInspection) harness.liveExampleExecution = execution;
        return { canvasId: canvas.id, duration: authored.duration, metrics: initial, endpoint };
      } finally {
        if (!retainForInspection) execution.terminate();
      }
    }, { source, objectCount, endpointTime, expectText, expectedFinalCenter, expectedComposition, filename });
    assert.equal(result.metrics.objectCount, objectCount, filename);
    assert.ok(result.metrics.drawCalls > 0, `${filename}: no draw calls`);
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
    if (expectText) {
      const pixels = textPixelStats(await page.locator(`#${result.canvasId}`).screenshot());
      assert.ok(pixels.count > 100, `${filename}: replacement glyphs were not rendered`);
      assert.ok(pixels.width > 50, `${filename}: replacement text has no glyph extent`);
      assert.ok(pixels.centerY < 180, `${filename}: replacement text lost its live position`);
    }
    if (endpointTime !== null || expectText || expectedFinalCenter !== null || expectedComposition) {
      await page.evaluate(() => {
        window.sharedAuthoringSmoke.liveExampleExecution.terminate();
        window.sharedAuthoringSmoke.liveExampleExecution = null;
      });
    }
  }

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
  console.log(
    `✓ shared authoring semantic execution rendered transferable/${transferable.backend} ` +
      `and shared/${shared.backend}; paired live membership and persisted Scene reuse rendered`,
  );
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
