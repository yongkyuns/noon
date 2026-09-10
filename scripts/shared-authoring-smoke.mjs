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

        # A detached handle created before the initially attached objects keeps
        # its semantic identity when live membership assigns a stable execution
        # slot. Neither admission nor removal may scan or checkpoint the whole
        # Python scene.
        class LocalKeys(dict):
            def values(self):
                raise AssertionError("typed binding scanned every object key")
        def reject_checkpoint(*_args, **_kwargs):
            raise AssertionError("typed binding checkpointed the whole scene")
        self._object_keys = LocalKeys(self._object_keys)
        self._authoring_checkpoint = reject_checkpoint
        next_id = self._next_object_id
        live.add(earlier)
        assert earlier.id == next_id
        assert earlier._scene is self
        live.remove(earlier)
        live.add(appended)
        assert appended.id == next_id + 1
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
        assert not hasattr(self, "_objects")
        self._objects = [{"poison": object()}]
        def reject_export(*_args, **_kwargs):
            raise AssertionError("semantic execution must not export legacy scene state")
        self.to_document = reject_export
        assert not hasattr(self, "to_scene_spec")
        assert not hasattr(self._canonical_authoring_context, "sceneSpecJson")
`;

const persistedSceneSource = `from noon import *
import builtins

scene = Scene()
circle = Circle(radius=1.0)
scene.add(circle)
# Empty/cleared updater metadata must not disable ordinary typed mutations.
circle.clear_updaters()
circle.shift(RIGHT)
assert circle.get_center() == (1.0, 0.0)
circle.shift(LEFT)
# A handle-less wrapper must fail before binding can project existing geometry
# into Python state. The same scene must remain usable afterward.
# Unsupported native bindings cannot append legacy declarations on an empty Scene.
unsupported = Scene()
for operation in ("bind_rotation", "bind_opacity", "bind_presence", "bind_position",
                  "bind_appearance", "bind_reveal", "bind_morph"):
    try:
        getattr(unsupported, operation)(None, object())
    except NotImplementedError:
        pass
    else:
        raise AssertionError(operation + " admitted a legacy binding")
assert not hasattr(unsupported, "_tracks")
color_probe = Circle().set_fill(BLUE, opacity=0.35).set_stroke(BLUE, opacity=0.2)
color_probe.set_color(GREEN)
assert abs(color_probe.get_fill_opacity() - 0.35) < 1e-6
assert abs(color_probe.get_stroke_opacity() - 0.2) < 1e-6
assert not hasattr(circle._semantic_handle, "wireTranslationX")
assert not hasattr(circle._semantic_handle, "wireFillRed")
assert not hasattr(circle._semantic_handle, "wireRotation")
rotation_probe = Circle()
angle = 0.123456789012345
rotation_probe._semantic_handle.setRotation(angle)
assert float(rotation_probe._semantic_handle.rotation) == angle
bindings_before = dict(scene._binding_handles)
next_id = scene._next_object_id
untyped = Circle(radius=0.2)
untyped._semantic_handle = None
try:
    untyped._bind_to_scene(scene)
except NotImplementedError as error:
    assert "requires a typed semantic Mobject" in str(error)
else:
    raise AssertionError("shared binding admitted a handle-less wrapper")
assert untyped._scene is None
assert scene._binding_handles == bindings_before
assert scene._next_object_id == next_id
assert len(scene._binding_handles) == 1
assert circle.get_center() == (0.0, 0.0)
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

function textBrightnessByRegion(buffer) {
  const png = PNG.sync.read(buffer);
  const splitX = png.width / 2 - 1.5 * png.height / 8;
  const result = { left: 0, right: 0 };
  for (let offset = 0; offset < png.data.length; offset += 4) {
    const brightness = Math.max(png.data[offset], png.data[offset + 1], png.data[offset + 2]);
    const pixel = offset / 4;
    const x = pixel % png.width;
    const y = Math.floor(pixel / png.width);
    if (brightness < 24 || y >= png.height / 2) continue;
    result[x < splitX ? "left" : "right"] += brightness;
  }
  return result;
}

function textBrightnessByBands(buffer) {
  const png = PNG.sync.read(buffer);
  const bands = [0, 0, 0];
  for (let offset = 0; offset < png.data.length; offset += 4) {
    const brightness = Math.max(png.data[offset], png.data[offset + 1], png.data[offset + 2]);
    if (brightness < 24) continue;
    const y = Math.floor(offset / 4 / png.width);
    bands[Math.min(2, Math.floor(3 * y / png.height))] += brightness;
  }
  return bands;
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

// Share source attachment across visual proofs; authored timing stays in Rust.
async function startSampledSource(page, source, canvasId, width = 640, height = 360) {
  console.log(`Checking sampled source ${canvasId}`);
  await page.evaluate(async ({ source, canvasId, width, height }) => {
    const harness = window.sharedAuthoringSmoke;
    const canvas = document.createElement("canvas");
    canvas.id = canvasId;
    canvas.width = width;
    canvas.height = height;
    document.body.append(canvas);
    let resolveAttached;
    let rejectAttached;
    const execution = new harness.AuthoringExecutionClient(canvas, {
      onError(error, owner) {
        const failure = new Error(`${canvasId} ${owner}: ${error}`);
        console.error(failure.message);
        rejectAttached(failure);
        execution.terminate();
      },
    });
    const attached = new Promise((resolve, reject) => {
      resolveAttached = resolve;
      rejectAttached = reject;
    });
    const authored = harness.authoring.run(source, {}, {
      async onSemanticContinuation(registration) {
        await execution.startSemanticExecution(registration.semanticExecution, {
          authoringClient: harness.authoring,
          loopDurationSeconds: Math.max(1, registration.duration),
          transportMode: "transferable",
          pacing: "external_samples",
        });
        resolveAttached();
      },
    });
    authored.then(() => rejectAttached(new Error(`${canvasId} did not register a continuation`)), (error) => {
      console.error(`${canvasId} authoring: ${error}`);
      rejectAttached(error);
      execution.terminate();
    });
    harness.sampledProof = { execution, authored };
    await attached;
  }, { source, canvasId, width, height });
  console.log(`Attached sampled source ${canvasId}`);
  await page.evaluate(() => window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(0));
  console.log(`Sampled ${canvasId} at 0s`);
}

async function stopSampledSource(page) {
  await page.evaluate(() => {
    window.sharedAuthoringSmoke.sampledProof.execution.terminate();
    window.sharedAuthoringSmoke.sampledProof = null;
  });
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
  const recordBrowserError = (error) => {
    browserErrors.push(error);
    console.error(error);
  };
  page.on("pageerror", (error) => recordBrowserError(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") recordBrowserError(`console: ${message.text()}`);
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
      filename: "manim_parity_uncreate_styled_square.py",
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
      filename: "manim_example_grow_from_point.py",
      objectCount: 5,
      expectedDuration: 4,
      endpointTime: null,
    },
    {
      filename: "manim_example_grow_from_center.py",
      objectCount: 2,
      expectedDuration: 2,
      endpointTime: null,
    },
    {
      filename: "manim_example_grow_from_edge.py",
      objectCount: 4,
      expectedDuration: 4,
      endpointTime: null,
    },
    {
      filename: "manim_example_spin_in_from_nothing.py",
      objectCount: 3,
      expectedDuration: 3,
      endpointTime: null,
    },
    {
      filename: "manim_parity_affine_lifecycle.py",
      objectCount: 0,
      expectedDuration: 2,
      endpointTime: null,
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
      filename: "manim_example_add_with_run_time.py",
      objectCount: 25,
      expectedDuration: 6,
      endpointTime: null,
    },
    {
      filename: "manim_example_succession.py",
      objectCount: 4,
      expectedDuration: 4,
      endpointTime: null,
    },
    { filename: "ordinary_filled_path_transform.py", objectCount: 1, expectedDuration: 3.2, endpointTime: null },
    { filename: "ordinary_family_membership_order.py", objectCount: 2, expectedDuration: 0.2, endpointTime: null },
    { filename: "ordinary_family_placement.py", objectCount: 3, expectedDuration: 1, endpointTime: null },
    { filename: "ordinary_dimension_fitting.py", objectCount: 2, expectedDuration: 0.2, endpointTime: null },
    { filename: "ordinary_family_replacement.py", objectCount: 3, expectedDuration: 0.2, endpointTime: null },
    { filename: "ordinary_family_affine.py", objectCount: 2, expectedDuration: 0.2, endpointTime: null },
    { filename: "ordinary_paint_queries_gradients.py", objectCount: 5, expectedDuration: 0.2, endpointTime: null },
    { filename: "ordinary_style_operations.py", objectCount: 3, expectedDuration: 0.2, endpointTime: null },
    { filename: "ordinary_family_paint.py", objectCount: 2, expectedDuration: 0.2, endpointTime: null },
    { filename: "ordinary_planar_affine.py", objectCount: 4, expectedDuration: 0.2, endpointTime: null },
    { filename: "ordinary_z_index.py", objectCount: 3, expectedDuration: 0.2, endpointTime: null },
    { filename: "ordinary_scale_pivots.py", objectCount: 4, expectedDuration: 0.2, endpointTime: null },
    { filename: "ordinary_family_grid.py", objectCount: 4, expectedDuration: 0.2, endpointTime: null },
    { filename: "ordinary_create_shapes.py", objectCount: 4, expectedDuration: 3.2, endpointTime: null },
    { filename: "ordinary_morph_stress.py", objectCount: 96, expectedDuration: 3.4, endpointTime: null },
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
          const diagnostic = JSON.stringify(
            latest,
            (_key, value) => typeof value === "bigint" ? value.toString() : value,
          );
          throw new Error(`live example did not render: ${diagnostic}`);
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
      assert.ok(
        Math.abs(result.duration - expectedDuration) < 1e-9,
        `${filename}: canonical live duration ${result.duration}, expected ${expectedDuration}`,
      );
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

  // Derived Python bookkeeping cannot become finalization authority or force
  // the shared scene into a document/export path.
  const typedFinalization = await page.evaluate(async () => {
    const result = await window.sharedAuthoringSmoke.authoring.run(`from noon import Circle, Scene
scene = Scene()
scene.add(Circle(radius=0.4))
assert scene._canonical_authoring_context is not None
assert not hasattr(scene._canonical_authoring_context, "checkpoint")
assert not hasattr(scene._canonical_authoring_context, "restore")
scene._binding_handles.clear()
def reject_export(*args, **kwargs):
    raise AssertionError("normal shared finalization invoked the document exporter")
scene.to_document = reject_export
assert not hasattr(scene, "to_scene_spec")
result = scene
`, {});
    return Object.hasOwn(result, "semanticExecution");
  });
  assert.equal(typedFinalization, true);

  // Top-level source and helper calls share the existing wait/play continuation.
  // Selecting result must not implicitly run its construct again.
  const topLevelSource = `from noon import *
class SelectedScene(Scene):
    def construct(self):
        raise AssertionError("prebuilt result construct ran twice")
result = SelectedScene()
for retired_state in ("_reactive_signals", "_reactive_bindings", "_reactive_signal_tracks", "_native_inputs"):
    assert not hasattr(result, retired_state), retired_state
# An obsolete Python cursor must never participate in shared time or admission.
result._cursor = object()
assert result.time == 0.0
def author(scene):
    scene.wait(0.25)
    assert scene.time == 0.25
    circle = Circle(0.4).set_fill(BLUE, opacity=1)
    scene.add(circle)
    scene.play(circle.animate.shift(RIGHT), run_time=0.5, rate_func=linear)
    assert abs(circle.get_center().x - 1) < 1e-6
    assert scene.time == 0.75
    scene.wait(0.25)
author(result)
`;
  await startSampledSource(page, topLevelSource, "scene-top-level-wait-play");
  try {
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(1), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 1);
    assert.equal(result.metrics.objectCount, 1);
  } finally {
    await stopSampledSource(page);
  }

  const topLevelExample = await readFile(
    path.join(repoRoot, "web/python/examples/top_level_family_arrangement.py"), "utf8",
  );
  await startSampledSource(page, topLevelExample, "scene-top-level-family-arrangement");
  try {
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(1), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 1);
    assert.equal(result.metrics.objectCount, 2);
  } finally {
    await stopSampledSource(page);
  }

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
  const primitiveConstructionSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_live_primitive_construction.py"),
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

  // A source-owned wait barrier keeps newly constructed primitives detached until
  // the continuation resumes. Their first rendered appearance must come from the
  // same publication that admits both Circle and Square.
  console.log("Checking live primitive construction after a wait");
  let primitiveTimeout;
  const primitiveConstruction = await Promise.race([page.evaluate(async (source) => {
    const harness = window.sharedAuthoringSmoke;
    const canvas = document.createElement("canvas");
    canvas.id = "scene-ordinary-live-primitive-construction";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    const execution = new harness.AuthoringExecutionClient(canvas);
    harness.primitiveConstructionExecution = execution;
    let resolveAttached;
    let rejectAttached;
    const attached = new Promise((resolve, reject) => {
      resolveAttached = resolve;
      rejectAttached = reject;
    });
    const authoredPromise = harness.authoring.run(source, {}, {
      async onSemanticContinuation(registration) {
        await execution.startSemanticExecution(registration.semanticExecution, {
          authoringClient: harness.authoring,
          transportMode: "transferable",
          pacing: "external_samples",
        });
        resolveAttached();
      },
    });
    authoredPromise.then(
      () => rejectAttached(new Error("primitive source returned without registering a continuation")),
      rejectAttached,
    );
    await attached;
    const before = (await execution.metrics()).metrics;
    if (before.objectCount !== 1) {
      throw new Error(`detached primitive construction published ${before.objectCount} objects before the wait barrier`);
    }
    const [, authored] = await Promise.all([
      execution.sampleToAuthoredTime(1),
      authoredPromise,
    ]);
    let after = null;
    for (let attempt = 0; attempt < 150; attempt += 1) {
      after = (await execution.metrics()).metrics;
      if (after.objectCount === 3 && after.presentedFrames > before.presentedFrames) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    return {
      canvasId: canvas.id,
      duration: authored.duration,
      before,
      after,
    };
  }, primitiveConstructionSource), new Promise((_, reject) => {
    primitiveTimeout = setTimeout(() => reject(new Error("live primitive construction did not complete within 45 seconds")), 45_000);
  })]).finally(() => clearTimeout(primitiveTimeout));
  assert.equal(primitiveConstruction.duration, 1);
  assert.equal(primitiveConstruction.before.objectCount, 1);
  assert.equal(primitiveConstruction.after.objectCount, 3);
  assert.ok(primitiveConstruction.after.drawCalls > 0);
  assert.ok(primitiveConstruction.after.presentedFrames > primitiveConstruction.before.presentedFrames);
  const primitivePixels = visiblePixelStats(
    await page.locator(`#${primitiveConstruction.canvasId}`).screenshot(),
  );
  assert.ok(primitivePixels.count > 500, "post-barrier primitive construction rendered a blank frame");
  await page.evaluate(() => {
    window.sharedAuthoringSmoke.primitiveConstructionExecution.terminate();
    window.sharedAuthoringSmoke.primitiveConstructionExecution = null;
  });

  const liveGeometrySource = await readFile(
    path.join(repoRoot, "web/python/examples/live_geometry_construction.py"), "utf8",
  );
  await startSampledSource(page, liveGeometrySource, "scene-shared-live-geometry");
  try {
    const initial = await page.evaluate(async () =>
      (await window.sharedAuthoringSmoke.sampledProof.execution.metrics()).metrics);
    assert.equal(initial.objectCount, 3);
    const final = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, result] = await Promise.all([execution.sampleToAuthoredTime(2), authored]);
      return { duration: result.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(final.duration, 2);
    assert.equal(final.metrics.objectCount, 9);
    const canvas = page.locator("#scene-shared-live-geometry");
    const blue = renderedWorldPixel(await canvas.screenshot(), -2, 0);
    const green = renderedWorldPixel(await canvas.screenshot(), 2, 1);
    const dot = renderedWorldPixel(await canvas.screenshot(), -4, -1.5);
    const annulus = renderedWorldPixel(await canvas.screenshot(), 4.375, -1.5);
    const underline = renderedWorldPixel(await canvas.screenshot(), 2, 0.45);
    assert.ok(dot.red > dot.green + 30, "late Dot lost its constructor color");
    assert.ok(annulus.red > annulus.blue + 30 && annulus.green > annulus.blue + 30,
      "late Annulus lost its constructor color");
    assert.ok(Math.min(underline.red, underline.green, underline.blue) > 150,
      "late Underline did not use the animated target bounds");
    assert.ok(blue.blue > blue.red + 30, "typed Path lost its blue fill");
    assert.ok(green.green > green.red + 30, "late Rectangle did not animate through shared live publication");
  } finally {
    await stopSampledSource(page);
  }

  const scaleSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_scale_in_place.py"), "utf8",
  );
  await startSampledSource(page, scaleSource, "scene-shared-scale-in-place", 960, 540);
  try {
    const duration = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(1), authored]);
      return completed.duration;
    });
    assert.equal(duration, 1);
    const edge = renderedWorldPixel(await page.locator("#scene-shared-scale-in-place").screenshot(), 1.8, 0);
    assert.ok(edge.blue > edge.red + 30, "ScaleInPlace did not capture the completed translation");
  } finally {
    await stopSampledSource(page);
  }

  const rotatingSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_rotating.py"), "utf8",
  );
  await startSampledSource(page, rotatingSource, "scene-shared-rotating", 960, 540);
  try {
    await page.evaluate(async () =>
      window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(0.75));
    const canvas = page.locator("#scene-shared-rotating");
    const screenshot = await canvas.screenshot();
    const cyan = (pixel) => pixel.green > pixel.red + 30 && pixel.blue > pixel.red + 30;
    assert.ok(cyan(renderedWorldPixel(screenshot, 2, 2)) && !cyan(renderedWorldPixel(screenshot, 3, 1)),
      "shared Rotating lost its deferred pivot or linear angular path");
    const duration = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(3.5), authored]);
      return completed.duration;
    });
    assert.equal(duration, 3.5);
    assert.ok(cyan(renderedWorldPixel(await canvas.screenshot(), 2, 2)),
      "shared Rotate lost its signed quarter-turn endpoint");
  } finally {
    await stopSampledSource(page);
  }

  const rotatingDefaultsSource = await readFile(
    path.join(repoRoot, "web/python/examples/manim_parity_rotating_centered.py"), "utf8",
  );
  await startSampledSource(page, rotatingDefaultsSource, "scene-shared-rotating-defaults", 960, 540);
  try {
    await page.evaluate(async () =>
      window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(0.625));
    const canvas = page.locator("#scene-shared-rotating-defaults");
    const diagonal = renderedWorldPixel(await canvas.screenshot(), 0.95, 0);
    assert.ok(diagonal.blue > diagonal.red + 30, "default Rotating did not follow its linear full-turn path");
    const duration = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(5), authored]);
      return completed.duration;
    });
    assert.equal(duration, 5);
  } finally {
    await stopSampledSource(page);
  }

  const focusSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_focus_on.py"), "utf8",
  );
  await startSampledSource(page, focusSource, "scene-shared-focus-on", 960, 540);
  try {
    await page.evaluate(async () =>
      window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(1.25));
    const canvas = page.locator("#scene-shared-focus-on");
    const screenshot = await canvas.screenshot();
    const middle = renderedWorldPixel(screenshot, 1, 0.5);
    const outside = renderedWorldPixel(screenshot, -6, 3);
    const cyan = (pixel) => pixel.green > pixel.red + 30 && pixel.blue > pixel.red + 30;
    assert.ok(cyan(middle) && !cyan(outside), "shared FocusOn did not shrink its spotlight");
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(2.5), authored]);
      return completed.duration;
    });
    assert.equal(result, 2.5);
    const final = await canvas.screenshot();
    assert.ok(!cyan(renderedWorldPixel(final, 1, 0.5)), "FocusOn left its spotlight visible");
    const square = renderedWorldPixel(final, -3, -2);
    assert.ok(square.blue > square.red + 30, "FocusOn removed unrelated scene content");
  } finally {
    await stopSampledSource(page);
  }

  const passingFlashSource = await readFile(
    path.join(repoRoot, "web/python/examples/line_passing_flash.py"), "utf8",
  );
  await startSampledSource(page, passingFlashSource, "scene-shared-line-passing-flash", 960, 540);
  try {
    const initial = await page.evaluate(async () =>
      (await window.sharedAuthoringSmoke.sampledProof.execution.metrics()).metrics);
    assert.equal(initial.objectCount, 0);
    await page.evaluate(async () =>
      window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(1.25));
    const canvas = page.locator("#scene-shared-line-passing-flash");
    const screenshot = await canvas.screenshot();
    const middle = renderedWorldPixel(screenshot, 0.5, -0.5);
    const outside = renderedWorldPixel(screenshot, -1.124, -1.4375);
    const cyan = (pixel) => pixel.green > pixel.red + 30 && pixel.blue > pixel.red + 30;
    assert.ok(cyan(middle) && !cyan(outside),
      "shared Line flash must render only its moving window at the midpoint");
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(2.5), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 2.5);
    assert.equal(result.metrics.objectCount, 1);
    const restored = renderedWorldPixel(await canvas.screenshot(), -1.124, -1.4375);
    assert.ok(cyan(restored), "re-added flash Line must restore its full original geometry");
  } finally {
    await stopSampledSource(page);
  }

  const becomeSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_become_semantics.py"), "utf8",
  );
  await startSampledSource(page, becomeSource, "scene-shared-become-semantics", 960, 540);
  try {
    const canvas = page.locator("#scene-shared-become-semantics");
    const initialScreenshot = await canvas.screenshot();
    const initialFitted = renderedWorldPixel(initialScreenshot, -2, 0);
    const initialStretched = renderedWorldPixel(initialScreenshot, 2, 0);
    const initialEllipse = renderedWorldPixel(initialScreenshot, 0, -2.5);
    assert.ok(initialFitted.blue > initialFitted.red + 30
        && initialStretched.blue > initialStretched.red + 30
        && initialEllipse.blue > initialEllipse.red + 30,
      "paired become sources must begin with their authored blue style");
    await page.evaluate(async () =>
      window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(0.5));
    const replacedScreenshot = await canvas.screenshot();
    const fittedCenter = renderedWorldPixel(replacedScreenshot, -2, 0);
    const fittedTall = renderedWorldPixel(replacedScreenshot, -2, 2);
    const stretchedCenter = renderedWorldPixel(replacedScreenshot, 2, 0);
    const stretchedWide = renderedWorldPixel(replacedScreenshot, 3.2, 0);
    const ellipseCenter = renderedWorldPixel(replacedScreenshot, 0, -2.5);
    const ellipseMajorAxis = renderedWorldPixel(replacedScreenshot, 1.3, -1.75);
    assert.ok(fittedCenter.red > fittedCenter.blue + 30
        && fittedCenter.green > fittedCenter.blue + 30
        && fittedTall.red > fittedTall.blue + 30
        && fittedTall.green > fittedTall.blue + 30,
      "height-then-width become did not publish the tall yellow target at the source center");
    assert.ok(stretchedCenter.red > stretchedCenter.green + 30
        && stretchedCenter.blue > stretchedCenter.green + 30
        && stretchedWide.red > stretchedWide.green + 30
        && stretchedWide.blue > stretchedWide.green + 30,
      "stretch become did not publish the wide magenta target at the source center");
    assert.ok(ellipseCenter.green > ellipseCenter.red + 30
        && ellipseCenter.blue > ellipseCenter.red + 30
        && ellipseMajorAxis.green > ellipseMajorAxis.red + 30
        && ellipseMajorAxis.blue > ellipseMajorAxis.red + 30,
      "rotated ellipse become did not preserve its cyan analytic geometry and style");
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(0.75), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 0.75);
    assert.equal(result.metrics.objectCount, 3,
      "detached become operands must not enter shared root membership");
  } finally {
    await stopSampledSource(page);
  }

  // The literal Manim Text remover starts detached. Shared Rust must admit it,
  // shrink about its visual center, and remove it at the source barrier.
  const shrinkSource = await readFile(
    path.join(repoRoot, "web/python/examples/manim_parity_shrink_to_center.py"), "utf8",
  );
  await startSampledSource(page, shrinkSource, "scene-shared-text-shrink");
  try {
    const canvas = page.locator("#scene-shared-text-shrink");
    const initial = visiblePixelStats(await canvas.screenshot(), (r, g, b) => Math.max(r, g, b) > 80);
    assert.ok(initial.count > 100, "detached Text was not admitted for Shrink");
    await page.evaluate(() => window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(0.5));
    const midpoint = visiblePixelStats(await canvas.screenshot(), (r, g, b) => Math.max(r, g, b) > 80);
    assert.ok(midpoint.count > 20 && midpoint.count < initial.count * 0.6,
      "Text did not shrink at the midpoint");
    assert.ok(Math.abs(midpoint.centerX - initial.centerX) < 4 &&
      Math.abs(midpoint.centerY - initial.centerY) < 4,
      "Text shrink moved its effective visual center");
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, result] = await Promise.all([execution.sampleToAuthoredTime(1), authored]);
      return { duration: result.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 1);
    assert.equal(result.metrics.objectCount, 0);
    assert.equal(visiblePixelStats(await canvas.screenshot(), (r, g, b) => Math.max(r, g, b) > 80).count, 0,
      "Text remained visible after shared Shrink completion");
  } finally {
    await stopSampledSource(page);
  }

  const compositionSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_timed_composition.py"), "utf8",
  );
  await startSampledSource(page, compositionSource, "scene-shared-timed-composition");
  try {
    const canvas = page.locator("#scene-shared-timed-composition");
    for (const [time, expected] of [
      [0, [true, false, false]],
      [0.4, [true, false, false]],
      [0.5, [true, true, false]],
      [0.6, [true, true, false]],
      [0.8, [true, true, true]],
      [1.8, [false, true, true]],
      [2.0, [false, false, true]],
      [2.25, [true, false, false]],
    ]) {
      await page.evaluate((time) => window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(time), time);
      const screenshot = await canvas.screenshot();
      const visible = [-2, 0, 2].map((x) => {
        const pixel = renderedWorldPixel(screenshot, x, 0);
        return pixel.blue > pixel.red + 25;
      });
      assert.deepEqual(visible, expected, `shared nested Add visibility at ${time}s`);
    }
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(2.5), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 2.5);
    assert.equal(result.metrics.objectCount, 1);
  } finally {
    await stopSampledSource(page);
  }

  const affineFadeSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_affine_fade.py"), "utf8",
  );
  await startSampledSource(page, affineFadeSource, "scene-shared-affine-fade");
  try {
    const canvas = page.locator("#scene-shared-affine-fade");
    for (const [time, x] of [[0.5, -1], [1, 0], [1.5, 1]]) {
      await page.evaluate(
        (sampleTime) => window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(sampleTime),
        time,
      );
      const color = renderedWorldPixel(await canvas.screenshot(), x, 0);
      assert.ok(
        color.blue > color.red + 25,
        `shared affine fade at ${time}s missed its Rust-resolved endpoint at ${x}`,
      );
    }
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(2), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 2);
    assert.equal(result.metrics.objectCount, 0);
  } finally {
    await stopSampledSource(page);
  }

  const mixedScalarSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_mixed_scalar_composition.py"), "utf8",
  );
  await startSampledSource(page, mixedScalarSource, "scene-shared-mixed-scalar");
  try {
    const canvas = page.locator("#scene-shared-mixed-scalar");
    for (const [time, x] of [
      [0, -2], [0.25, -2], [0.5, -2], [1, -1.7195852],
      [1.5, 0], [2.5, 2], [2.75, 1],
    ]) {
      await page.evaluate((time) => window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(time), time);
      const screenshot = await canvas.screenshot();
      for (const offset of [-0.15, 0.15]) {
        const color = renderedWorldPixel(screenshot, x + offset, 1);
        assert.ok(color.blue > color.red + 25, `mixed scalar composition at ${time}s: circle expected at ${x}`);
      }
    }
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(3), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 3);
    assert.equal(result.metrics.objectCount, 2);
  } finally {
    await stopSampledSource(page);
  }

  const arrangedOptionsSource = `from noon import *
class ArrangedOptions(Scene):
    def construct(self):
        first = Circle(0.2).set_fill(BLUE, opacity=1)
        second = Circle(0.2).shift(2 * RIGHT)
        nested = VGroup(first, second)
        family = VGroup(first, nested)
        left = VGroup(first)
        right = VGroup(second)
        empty = VGroup()
        selected = VGroup(left, right)
        invalid = VGroup(left, right, empty)
        def reject_python_placement(*args, **kwargs):
            raise AssertionError("arrange sequenced Python member placements")
        for member in (first, second, nested, left, right, empty):
            member.next_to = reject_python_placement
            member.get_critical_point = reject_python_placement
        family.shift(RIGHT)
        assert abs(first.get_center().x - 1) < 1e-6
        assert abs(second.get_center().x - 3) < 1e-6
        family.arrange(RIGHT, buff=0.2, aligned_edge=UP)
        assert abs(first.get_center().x + 1) < 1e-6
        assert abs(second.get_center().x - 1) < 1e-6
        self.add(first, second)
        self.wait(0.1)
        try:
            invalid.arrange(center=False, index_of_submobject_to_align=0)
        except NoonValueError as error:
            assert error.category == "invalid_input"
            assert (error.rust_cause or error).code == "authoring.invalid_submobject_index"
        else:
            raise AssertionError("late invalid arrangement index was accepted")
        assert abs(first.get_center().x + 1) < 1e-6
        assert abs(second.get_center().x - 1) < 1e-6
        selected.arrange(RIGHT, buff=0.5, center=False,
                         index_of_submobject_to_align=-1, submobject_to_align=second)
        assert abs(first.get_center().x + 1) < 1e-6
        assert abs(second.get_center().x + 0.1) < 1e-6
        self.wait(0.1)
`;
  await startSampledSource(page, arrangedOptionsSource, "scene-arrange-options");
  try {
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(0.2), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 0.2);
    assert.equal(result.metrics.objectCount, 2);
  } finally {
    await stopSampledSource(page);
  }

  const selectedAlignmentSource = `from noon import *
class SelectedAlignment(Scene):
    def construct(self):
        first = Square(1).set_fill(BLUE, opacity=1)
        second = Square(1).shift(2 * RIGHT)
        nested = VGroup(second)
        family = VGroup(first, nested)
        target = VGroup(Square(2).shift(6 * RIGHT))
        def reject_python_bounds(*args, **kwargs):
            raise AssertionError("selected placement evaluated Python critical points")
        for value in (first, second, nested, family, target, target[0]):
            value.get_critical_point = reject_python_bounds
        family.next_to(target, index_of_submobject_to_align=0,
                       submobject_to_align=second, buff=0.25)
        assert abs(first.get_center().x - 5.75) < 1e-6
        assert abs(second.get_center().x - 7.75) < 1e-6
        self.add(first, second, target)
        self.wait(0.1)
        family.next_to(ORIGIN, index_of_submobject_to_align=-1, buff=0.25)
        assert abs(first.get_center().x + 1.25) < 1e-6
        assert abs(second.get_center().x - 0.75) < 1e-6
        try:
            family.next_to(ORIGIN, index_of_submobject_to_align=-3)
        except NoonValueError as error:
            assert error.category == "invalid_input"
            assert (error.rust_cause or error).code == "authoring.invalid_submobject_index"
        else:
            raise AssertionError("invalid family index was accepted")
        assert abs(first.get_center().x + 1.25) < 1e-6
        first.next_to(2 * RIGHT, submobject_to_align=second, buff=0.25)
        assert abs(first.get_center().x - 0.75) < 1e-6
        assert abs(second.get_center().x - 0.75) < 1e-6
        self.wait(0.1)
`;
  await startSampledSource(page, selectedAlignmentSource, "scene-selected-alignment");
  try {
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(0.2), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 0.2);
    assert.equal(result.metrics.objectCount, 3);
  } finally {
    await stopSampledSource(page);
  }

  const familyStateSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_family_state.py"), "utf8",
  );
  await startSampledSource(page, familyStateSource, "scene-shared-family-state");
  try {
    const canvas = page.locator("#scene-shared-family-state");
    for (const [time, leftX, rightX, y] of [[0, -1, 1, 0], [0.2, -0.5, 1.5, 0.5], [0.4, -2, 0, 0], [0.6000000000000001, -2, 0, 0]]) {
      await page.evaluate(time => window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(time), time);
      const frame = await canvas.screenshot();
      const left = renderedWorldPixel(frame, leftX, y);
      const right = renderedWorldPixel(frame, rightX, y);
      assert.ok(left.red > 180 && left.blue < 50, `family state at ${time}s missed red member`);
      assert.ok(right.blue > 180 && right.red < 50, `family state at ${time}s missed blue member`);
    }
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const completed = await authored;
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.ok(Math.abs(result.duration - 0.6) < 1e-9);
    assert.equal(result.metrics.objectCount, 2);
  } finally {
    await stopSampledSource(page);
  }

  const familyTransformIndicateSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_family_transform_indicate.py"), "utf8",
  );
  await startSampledSource(page, familyTransformIndicateSource, "scene-shared-family-transform-indicate");
  try {
    const canvas = page.locator("#scene-shared-family-transform-indicate");
    for (const [time, leftX, rightX, highlighted] of [
      [0, -2, 0], [0.5, -1.625, 0], [1, -1.25, 0.25], [2, -1, 1],
      [8 / 3, -1.2, 1, "left"], [10 / 3, -1, 1.2, "right"],
      [4, -1, 1], [4.25, 0, 1], [4.5, 0, 1],
    ]) {
      await page.evaluate(
        (sampleTime) => window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(sampleTime),
        time,
      );
      const screenshot = await canvas.screenshot();
      const left = renderedWorldPixel(screenshot, leftX, 0);
      const right = renderedWorldPixel(screenshot, rightX, 0);
      assert.ok(Math.max(left.red, left.green, left.blue) > 80,
        `family transform/Indicate at ${time}s missed left member at ${leftX}`);
      assert.ok(Math.max(right.red, right.green, right.blue) > 80,
        `family transform/Indicate at ${time}s missed right member at ${rightX}`);
      if (highlighted !== undefined) {
        const color = highlighted === "left" ? left : right;
        assert.ok(color.red > 180 && color.green > 150 && color.blue < color.green - 40,
          `family Indicate ${highlighted} member did not reach yellow outward state`);
      }
      if (time === 4 || time === 4.5) {
        assert.ok(left.red > left.green + 40 && left.blue > left.green + 40,
          "family Indicate did not restore the transformed pink left member");
        assert.ok(right.blue > right.red + 25,
          "family Indicate did not restore the transformed blue right member");
      }
    }
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const completed = await authored;
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 4.5);
    assert.equal(result.metrics.objectCount, 2);
  } finally {
    await stopSampledSource(page);
  }

  const drawBorderThenFillSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_draw_border_then_fill.py"), "utf8",
  );
  // Match the direct proof's resolution so the thin outline covers a full pixel.
  await startSampledSource(page, drawBorderThenFillSource, "scene-shared-draw-border-then-fill", 960, 540);
  try {
    const canvas = page.locator("#scene-shared-draw-border-then-fill");
    await page.evaluate(() => window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(0.5));
    const outlineFrame = await canvas.screenshot();
    const outline = renderedWorldPixel(outlineFrame, -1, 0.4);
    const unfilled = renderedWorldPixel(outlineFrame, -1, 0);
    assert.ok(Math.abs(outline.red - 247) < 15 && Math.abs(outline.green - 217) < 15 && Math.abs(outline.blue - 111) < 15,
      `shared DrawBorderThenFill must reveal the yellow outline in phase one: ${JSON.stringify(outline)}`);
    assert.ok(Math.max(unfilled.red, unfilled.green, unfilled.blue) < 40,
      "shared DrawBorderThenFill must keep fill transparent in phase one");
    for (const [time, x] of [[1.5, -1], [2, -1], [2.5, 1], [3, 1]]) {
      await page.evaluate(
        (sampleTime) => window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(sampleTime),
        time,
      );
      const color = renderedWorldPixel(await canvas.screenshot(), x, 0);
      assert.ok(color.alpha > 40 && color.red > 90,
        `shared DrawBorderThenFill at ${time}s missed filled member at ${x}`);
    }
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(3.25), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 3.25);
    assert.equal(result.metrics.objectCount, 2);
  } finally {
    await stopSampledSource(page);
  }

  const textWriteSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_text_write.py"), "utf8",
  );
  await startSampledSource(page, textWriteSource, "scene-shared-text-write", 960, 540);
  try {
    const canvas = page.locator("#scene-shared-text-write");
    const samples = [];
    for (const time of [0, 0.5, 1, 2, 2.5, 3]) {
      // Attachment has already sampled zero; inspect that published frame directly.
      if (time !== 0) {
        console.log(`Sampling Text Write at ${time}s`);
        await page.evaluate(async (sampleTime) => {
          const { execution } = window.sharedAuthoringSmoke.sampledProof;
          let timer;
          try {
            return await Promise.race([
              execution.sampleToAuthoredTime(sampleTime),
              new Promise((_, reject) => {
                timer = setTimeout(() => reject(new Error(`Text Write sample ${sampleTime}s timed out`)), 15000);
              }),
            ]);
          } finally {
            clearTimeout(timer);
          }
        }, time);
      }
      const stats = visiblePixelStats(await canvas.screenshot(),
        (red, green, blue, _x, y) => y < 270 && red > 100 && green > 100 && blue > 100);
      samples.push(stats.count);
    }
    assert.equal(samples[0], 0, "Text Write starts with hidden glyphs");
    assert.ok(samples[1] > 0 && samples[2] > samples[1] && samples[3] > samples[1]
        && samples[4] > 0 && samples[4] < samples[3] && samples[5] === 0,
      `Text Write must reveal glyph outline/fill phases: ${JSON.stringify(samples)}`);
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(3.25), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 3.25);
    assert.equal(result.metrics.objectCount, 1);
  } finally {
    await stopSampledSource(page);
  }

  const textFamilyWriteSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_text_family_write.py"), "utf8",
  );
  await startSampledSource(
    page, textFamilyWriteSource, "scene-shared-text-family-write", 960, 540,
  );
  try {
    const canvas = page.locator("#scene-shared-text-family-write");
    const samples = [];
    for (const time of [0, 0.25, 1, 2, 2.5, 3]) {
      if (time !== 0) {
        await page.evaluate(async (sampleTime) => {
          await window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(sampleTime);
        }, time);
      }
      samples.push({ time, ...textBrightnessByRegion(await canvas.screenshot()) });
    }
    assert.equal(samples[0].left, 0);
    assert.equal(samples[0].right, 0);
    assert.ok(samples[1].left > 0 && samples[1].right === 0,
      `family Write must begin with the first Text leaf: ${JSON.stringify(samples)}`);
    assert.ok(samples[2].left > 0 && samples[2].right > 0);
    assert.ok(samples[3].left > 0 && samples[3].right > samples[2].right);
    assert.ok(samples[4].left > 0 && samples[4].right > 0
        && samples[4].right < samples[3].right,
      `family Unwrite must erase the four-glyph leaf before the one-glyph leaf: ${JSON.stringify(samples)}`);
    assert.ok(samples[5].left === 0 && samples[5].right === 0);
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(3.25), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 3.25);
    assert.equal(result.metrics.objectCount, 1);
  } finally {
    await stopSampledSource(page);
  }

  const exactPropertySource = await readFile(
    path.join(repoRoot, "web/python/examples/exact_property_tracks.py"), "utf8",
  );
  await startSampledSource(page, exactPropertySource, "scene-shared-exact-property", 960, 540);
  try {
    const canvas = page.locator("#scene-shared-exact-property");
    const samples = [];
    for (const time of [0, 1, 2]) {
      if (time !== 0) {
        await page.evaluate(async (sampleTime) => {
          await window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(sampleTime);
        }, time);
      }
      const screenshot = await canvas.screenshot();
      const circle = renderedWorldPixel(screenshot, -2 + 2 * time, 1);
      const square = renderedWorldPixel(screenshot, 0, -1);
      assert.ok(circle.red > circle.green + 30 && square.blue > square.red + 100,
        `paired exact-track endpoints must render at ${time}: ${JSON.stringify({ circle, square })}`);
      samples.push(circle.red);
    }
    assert.ok(samples[0] > samples[1] && samples[1] > samples[2],
      `paired exact-track opacity must decrease: ${JSON.stringify(samples)}`);
  } finally {
    await stopSampledSource(page);
  }

  const specializedGeometrySource = await readFile(
    path.join(repoRoot, "web/python/examples/specialized_geometry.py"), "utf8",
  );
  await startSampledSource(page, specializedGeometrySource, "scene-shared-specialized-geometry", 960, 540);
  try {
    const screenshot = await page.locator("#scene-shared-specialized-geometry").screenshot();
    const dot = renderedWorldPixel(screenshot, -4, 2);
    const rectangle = renderedWorldPixel(screenshot, -4, 0);
    assert.ok(dot.blue > dot.red + 30 && rectangle.blue > rectangle.red + 30,
      `typed specialized geometry must render the paired style: ${JSON.stringify({ dot, rectangle })}`);
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(1), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 1);
    assert.equal(result.metrics.objectCount, 9);
  } finally {
    await stopSampledSource(page);
  }

  const automaticWaitTextSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_automatic_wait_text.py"), "utf8",
  );
  await startSampledSource(
    page, automaticWaitTextSource, "scene-shared-automatic-wait-text", 960, 540,
  );
  try {
    const canvas = page.locator("#scene-shared-automatic-wait-text");
    const samples = [];
    for (const time of [0, 0.25, 0.5, 1, 1.5, 1.75, 2]) {
      if (time !== 0) {
        await page.evaluate(async (sampleTime) => {
          await window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(sampleTime);
        }, time);
      }
      const bands = textBrightnessByBands(await canvas.screenshot());
      const metrics = await page.evaluate(async () =>
        (await window.sharedAuthoringSmoke.sampledProof.execution.metrics()).metrics);
      samples.push({ time, bands, brightness: bands.reduce((sum, value) => sum + value, 0),
        objects: metrics.objectCount });
    }
    assert.deepEqual(samples.slice(0, 2).map(({ brightness, objects }) => [brightness, objects]),
      [[0, 0], [0, 0]], "initial wait must remain an empty shared execution");
    assert.equal(samples[2].brightness, 0);
    assert.equal(samples[2].objects, 3);
    assert.ok(samples[3].bands.every((value) => value > 0) && samples[3].objects === 3,
      `late Text, Typst, and MathTypst must all render during FadeIn: ${JSON.stringify(samples)}`);
    assert.ok(samples[4].bands.every((value) => value > 0)
        && samples[4].brightness > samples[3].brightness && samples[4].objects === 3);
    assert.ok(samples[5].brightness > 0 && samples[5].brightness < samples[4].brightness
        && samples[5].objects === 3);
    assert.equal(samples[6].brightness, 0);
    assert.equal(samples[6].objects, 0);
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(2.25), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 2.25);
    assert.equal(result.metrics.objectCount, 0);
  } finally {
    await stopSampledSource(page);
  }

  const textFamilyRevealSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_text_family_reveal.py"), "utf8",
  );
  await startSampledSource(
    page, textFamilyRevealSource, "scene-shared-text-family-reveal", 960, 540,
  );
  try {
    const canvas = page.locator("#scene-shared-text-family-reveal");
    const samples = [];
    for (const time of [0, 0.25, 1, 2, 2.5, 3]) {
      if (time !== 0) {
        await page.evaluate(async (sampleTime) => {
          await window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(sampleTime);
        }, time);
      }
      samples.push({ time, ...textBrightnessByRegion(await canvas.screenshot()) });
    }
    assert.equal(samples[0].left, 0);
    assert.equal(samples[0].right, 0);
    assert.ok(samples[1].left > 0 && samples[1].right === 0,
      `family Create must begin with the first Text leaf: ${JSON.stringify(samples)}`);
    assert.ok(samples[2].left > 0 && samples[2].right > 0);
    assert.ok(samples[3].left > 0 && samples[3].right > samples[2].right);
    assert.ok(samples[4].left === 0 && samples[4].right > 0
        && samples[4].right < samples[3].right,
      `family Uncreate must reverse local reveal without reversing leaf order: ${JSON.stringify(samples)}`);
    assert.ok(samples[5].left === 0 && samples[5].right === 0);
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(3.25), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 3.25);
    assert.equal(result.metrics.objectCount, 1);
  } finally {
    await stopSampledSource(page);
  }

  const membershipSource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_membership.py"), "utf8",
  );
  await startSampledSource(page, membershipSource, "scene-shared-membership");
  try {
    const canvas = page.locator("#scene-shared-membership");
    for (const [stage, count] of [2, 2, 1, 3, 2, 2, 0].entries()) {
      const time = stage * 0.5 + 0.25;
      const metrics = await page.evaluate(async (time) => {
        const execution = window.sharedAuthoringSmoke.sampledProof.execution;
        await execution.sampleToAuthoredTime(time);
        return (await execution.metrics()).metrics;
      }, time);
      assert.equal(metrics.objectCount, count, `shared membership stage ${stage}`);
      const color = renderedWorldPixel(await canvas.screenshot(), 0, 0);
      const channel = stage === 2 || stage === 5 ? "green" : "blue";
      if (count > 0) {
        assert.ok(color[channel] > 100 && color[channel] > color.red + 40,
          `shared membership painter order at stage ${stage}: ${JSON.stringify(color)}`);
      }
    }
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(3.5), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 3.5);
    assert.equal(result.metrics.objectCount, 0);
  } finally {
    await stopSampledSource(page);
  }

  const subsetDisplaySource = await readFile(
    path.join(repoRoot, "web/python/examples/ordinary_subset_display.py"), "utf8",
  );
  await startSampledSource(page, subsetDisplaySource, "scene-shared-subset-display");
  try {
    const canvas = page.locator("#scene-shared-subset-display");
    for (const [time, rows] of [
      [0.5, [[0.7, []]]],
      [1, [[0.7, [-1]]]],
      [2, [[0.7, [-1, 0]]]],
      [3, [[0.7, [-1, 0, 1]], [-0.7, []]]],
      [3.5, [[-0.7, [-1]]]],
      [4, [[-0.7, [-1]]]],
      [4.5, [[-0.7, [0]]]],
      [5, [[-0.7, [0]]]],
      [5.5, [[-0.7, [1]]]],
      [6, [[-0.7, [1]]]],
    ]) {
      console.log(`Sampling shared subset display at ${time}s`);
      await page.evaluate(
        (sampleTime) => window.sharedAuthoringSmoke.sampledProof.execution.sampleToAuthoredTime(sampleTime),
        time,
      );
      const pixels = await canvas.screenshot();
      for (const [row, expected] of rows) {
        for (const x of [-1, 0, 1]) {
          const color = renderedWorldPixel(pixels, x, row);
          const visible = Math.max(color.red, color.green, color.blue) > 70;
          assert.equal(visible, expected.includes(x),
            `shared subset display threshold mismatch at ${time}s, x=${x}: ${JSON.stringify(color)}`);
        }
      }
      console.log(`Sampled shared subset display at ${time}s`);
    }
    console.log("Completing shared subset display at 6.25s");
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(6.25), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    console.log("Completed shared subset display at 6.25s");
    assert.equal(result.duration, 6.25);
    assert.equal(result.metrics.objectCount, 6);
  } finally {
    await stopSampledSource(page);
  }

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

  // A top-level wait and later scalar play use the same shared Rust cursor.
  const topLevelScalarSource = `from noon import Circle, Scene, linear
scene = Scene()
circle = Circle(radius=0.4)
scene.add(circle)
progress = scene.value_tracker(0.0)
scene.wait(1.0)
assert scene.time == 1.0
scene.play(progress.animate(run_time=2.0, rate_func=linear).set_value(4.0))
assert scene.time == 3.0
assert progress.get_value() == 4.0
result = scene
`;
  await startSampledSource(page, topLevelScalarSource, "scene-top-level-wait-scalar");
  try {
    const result = await page.evaluate(async () => {
      const { execution, authored } = window.sharedAuthoringSmoke.sampledProof;
      const [, completed] = await Promise.all([execution.sampleToAuthoredTime(3), authored]);
      return { duration: completed.duration, metrics: (await execution.metrics()).metrics };
    });
    assert.equal(result.duration, 3);
    assert.equal(result.metrics.objectCount, 1);
  } finally {
    await stopSampledSource(page);
  }

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
  assert.ok(Math.abs(paintStyle.stroke.alpha - 0.4) < 1e-6);
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

    await rasterPage.reload({ waitUntil: "load" });
    await rasterPage.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
    const rotating = await rasterPage.evaluate(async (source) => {
      await window.noonHostRaster.ready();
      return window.noonHostRaster.load(source, 6);
    }, rotatingDefaultsSource);
    assert.equal(rotating.kind, "semantic_execution");
    assert.equal(rotating.objectCount, 1);
    const rotationTimes = [0, 0.625, 5];
    await rasterPage.evaluate((times) => window.noonHostRaster.renderThrough(1, times), rotationTimes);
    const angularCapture = await rasterPage.evaluate(() => window.noonHostRaster.debugFrame());
    assert.equal(angularCapture.time, 0.625);
    assert.ok(angularCapture.publication);
    assert.equal(angularCapture.objects.length, 1);
    assert.ok(Math.abs(angularCapture.objects[0].transform.rotation - Math.PI / 4) < 1e-6);
    const diagonal = renderedWorldPixel(await rasterPage.locator("#scene").screenshot(), 0.95, 0);
    assert.ok(diagonal.blue > diagonal.red + 30, "raster host did not sample the five-second angular path");
    const rotated = await rasterPage.evaluate((times) => window.noonHostRaster.renderThrough(2, times), rotationTimes);
    assert.equal(rotated.time, 5);
    assert.equal(rotated.authoredDuration, 5);
    assert.equal(rotated.objectCount, 1);
    assert.equal(rotated.presented, true);
    const completedCapture = await rasterPage.evaluate(() => window.noonHostRaster.debugFrame());
    assert.equal(completedCapture.time, 5);
    assert.equal(completedCapture.present_object_count, 1);

    await rasterPage.reload({ waitUntil: "load" });
    await rasterPage.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
    await rasterPage.evaluate(async () => {
      await window.noonHostRaster.ready();
      await window.noonHostRaster.load(`from noon import *
class RejectedAdmission(Scene):
    def construct(self):
        entering = Square()
        try:
            self.play(Create(entering), object())
            raise AssertionError("unsupported composition was accepted")
        except NotImplementedError:
            pass
        assert entering not in self.mobjects
        self.play(Create(entering), run_time=0.1)
        self.play(Uncreate(entering), run_time=0.1)
        assert entering not in self.mobjects
`, 1);
    });
    const admitted = await rasterPage.evaluate(() => window.noonHostRaster.renderThrough(2, [0, 0.1, 0.2]));
    assert.equal(admitted.time, 0.2);
    assert.equal(admitted.objectCount, 0);
    assert.equal(admitted.authoredDuration, 0.2);

    await rasterPage.reload({ waitUntil: "load" });
    await rasterPage.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
    await rasterPage.evaluate(async () => {
      await window.noonHostRaster.ready();
      await window.noonHostRaster.load(`from noon import *
class LateFailure(Scene):
    def construct(self):
        self.add(Circle())
        self.wait(0.1)
        raise ValueError("intentional raster continuation failure")
`, 1);
    });
    await assert.rejects(
      rasterPage.evaluate(() => window.noonHostRaster.renderThrough(1, [0, 0.1])),
      /intentional raster continuation failure/,
      "a failed source continuation must reject its pending sample with the original error",
    );
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
