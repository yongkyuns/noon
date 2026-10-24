// Real public Python -> canonical Rust authoring -> typed historical canvas proof.
// Lifecycle and observations cross JavaScript; no scene/history is retransmitted.
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { PYTHON_COMPAT_MODULES } from "../web/python-compat-modules.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_PREPARED_PYTHON_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/prepared-canvas/python");
const worker = await readFile(path.join(root, "web/python-worker.source.js"), "utf8");
const pyodideUrl = worker.match(/import \{ loadPyodide \} from "([^"]+)";/)?.[1];
assert.ok(pyodideUrl, "use the product's pinned interpreter");
const modules = await Promise.all(PYTHON_COMPAT_MODULES.map(async ({sourcePath, runtimePath}) => ({
  runtimePath, source: await readFile(path.join(root, "web", sourcePath), "utf8"),
})));
const cache = createPyodideResourceCache(worker);
const report = { status: "running", pyodideUrl, cases: [] };
await mkdir(output, { recursive: true });
let server, browser;
function changedPixels(a, b) {
  assert.equal(a.width, b.width); assert.equal(a.height, b.height);
  let changed = 0;
  for (let i = 0; i < a.data.length; i += 4) {
    if (!a.data.subarray(i, i + 4).equals(b.data.subarray(i, i + 4))) changed++;
  }
  return changed;
}
try {
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    const result = { backend, status: "running", checks: [], console: [], errors: [] };
    report.cases.push(result);
    browser = await chromium.launch({ headless: true, args: browserArgs(backend), channel: "chromium" });
    const context = await browser.newContext({ viewport: { width: 900, height: 600 } });
    await cache.install(context);
    const page = await context.newPage(); page.setDefaultTimeout(180_000);
    page.on("console", message => result.console.push(message.text()));
    page.on("pageerror", error => result.errors.push(String(error)));
    const state = () => page.evaluate(() => JSON.parse(preparedPython.renderer.debugPreparedPlaybackJson()));
    const receipt = () => page.evaluate(() => JSON.parse(preparedPython.renderer.debugPointerPresentationJson()));
    const capture = async label => {
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const bytes = await page.locator("#scene").screenshot();
      await writeFile(path.join(output, `${backend}-${label}.png`), bytes);
      return PNG.sync.read(bytes);
    };
    const step = time => page.evaluate(time => {
      preparedPython.renderer.advanceDirectRealtime(time); preparedPython.renderer.render();
    }, time);
    const click = async () => {
      const rect = await page.locator("#scene").boundingBox();
      await page.mouse.click(rect.x + 320, rect.y + 180);
    };
    try {
      await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
      await page.evaluate(async ({modules, pyodideUrl}) => {
        globalThis.Worker = class { constructor() { throw new Error("same-context prefix test attempted worker transport"); } };
        const wasm = await import("./pkg/noon_web.js"); await wasm.default();
        const { loadPyodide } = await import(pyodideUrl);
        const pyodide = await loadPyodide();
        const store = new wasm.WasmAuthoringStore();
        // Same thin constructor/options projections used by the production worker.
        globalThis.noonCreateCanonicalAuthoringSceneContext = () => store.createSceneContext();
        globalThis.noonCreateAuthoringValueTrackerHandle = value => store.createValueTracker(value);
        globalThis.noonAuthoringGeometryOptions = wasm.WasmManimGeometryOptions;
        globalThis.noonAuthoringVectorPath = () => new wasm.WasmAuthoringVectorPath();
        globalThis.noonCreateAuthoringGeometryHandle = options => store.createManimGeometry(options);
        globalThis.noonAuthoringMembershipBatch = kind => new wasm.WasmSceneMembershipBatch(kind);
        globalThis.noonCreateAuthoringFamilyHandle = (batch, zIndex, drawEnabled, ...color) => color.length
          ? store.createFamilyWithColor(batch, zIndex, ...color, drawEnabled) : store.createFamily(batch, zIndex, drawEnabled);
        globalThis.noonResolveAnimationOptions = (...args) => {
          const value = wasm.resolveAnimationOptions(...args);
          try { return { runTime: value.runTime, rateFunc: value.rateFunc, lagRatio: value.lagRatio,
            pathArc: value.pathArc, reverseRateFunction: value.reverseRateFunction }; }
          finally { value.free(); }
        };
        for (const {runtimePath, source} of modules) pyodide.FS.writeFile(runtimePath, source);
        await pyodide.runPythonAsync(`
import sys
sys.path.insert(0, "/tmp")
from noon import Scene, Circle, Indicate, BLUE, RIGHT
scene = Scene()
target = Circle(radius=0.8, color=BLUE, fill_opacity=1.0, stroke_width=0.0)
scene.add(target)
scene.on_click(target, Indicate(target, run_time=0.5))
endpoint = target.copy().shift(RIGHT * 2)
animation = scene.declare_live_transform_to(target, endpoint, run_time=1.0, rate_func="linear")
live = scene.live_execution()
assert live.wait(1.0) == 1.0
assert live.prepare() == 1.0
source_context = scene._canonical_authoring_context
assert source_context.liveExecutionOwnership() == "active"
`);
        const source = pyodide.globals.get("source_context");
        const bridge = new wasm.InterpreterCanvasFixture(source);
        const canvas = document.querySelector("#scene");
        const renderer = await bridge.createCanvas(canvas.transferControlToOffscreen());
        renderer.setPreparedPlaying(false);
        const { attachNativeInputs } = await import("./native-inputs.js");
        const errors = [];
        globalThis.preparedPython = { pyodide, store, source, bridge, renderer, errors };
        preparedPython.detach = attachNativeInputs(renderer, canvas, {
          inspectionZoom: true, onError: error => errors.push(String(error)),
        });
        renderer.render();
      }, { modules, pyodideUrl });
      assert.equal(await page.evaluate(() => preparedPython.renderer.rendererBackend()), backend === "webgpu" ? "WebGPU" : "WebGL2");
      assert.equal((await state()).time, 0.5); assert.equal((await state()).frontier, 1); assert.equal((await state()).end, null);
      const baseline = await capture("initial");
      assert.ok(baseline.data.some((value, i) => i % 4 === 2 && value > 150), "nonempty source pixels required");
      result.checks.push("public-python-headless-wait", "independent-earlier-reader", "actual-backend");

      await click(); await step(1000); await step(1250);
      result.highlightPixels = changedPixels(baseline, await capture("highlight"));
      assert.ok(result.highlightPixels > 500);
      await step(1500);
      assert.equal(changedPixels(baseline, await capture("restored")), 0);
      result.checks.push("historical-python-click", "exact-effect-restoration");

      const rect = await page.locator("#scene").boundingBox();
      await page.mouse.move(rect.x + 320, rect.y + 180); await page.mouse.down();
      const before = await receipt();
      await page.evaluate(async () => {
        await preparedPython.pyodide.runPythonAsync(`
assert live.play(animation) == 2.0
assert live.prepare() == 2.0
assert abs(live.effective_center(target).x - 2.0) < 1e-6
assert abs(target.get_center().x - 2.0) < 1e-6
assert source_context.liveExecutionOwnership() == "active"
`);
        preparedPython.bridge.publishCompleted(preparedPython.source, preparedPython.renderer, false);
      });
      assert.equal((await state()).frontier, 2); assert.equal((await state()).time, 0.5);
      assert.deepEqual(await receipt(), before);
      assert.equal(changedPixels(baseline, await capture("future-delivered")), 0);
      await page.mouse.up(); await step(2000); await step(2250);
      assert.ok(changedPixels(baseline, await capture("after-delivery-highlight")) > 500);
      await step(2500);
      assert.equal(changedPixels(baseline, await capture("after-delivery-restored")), 0);
      result.checks.push("python-animation-completion-observed", "future-delivery-preserves-press-and-receipt");

      await page.evaluate(() => { preparedPython.renderer.seekDirect(1.5); preparedPython.renderer.render(); });
      const halfway = await capture("animation-midpoint");
      assert.ok(changedPixels(baseline, halfway) > 500);
      await page.evaluate(() => { preparedPython.renderer.seekDirect(0.5); preparedPython.renderer.render(); });
      assert.equal(changedPixels(baseline, await capture("rewound")), 0);
      await page.evaluate(async () => {
        await preparedPython.pyodide.runPythonAsync(`live.set_translation(target, 3.0, 0.0)`);
        preparedPython.bridge.publishCompleted(preparedPython.source, preparedPython.renderer, true);
        await preparedPython.pyodide.runPythonAsync(`
del live, animation, endpoint, target, scene, source_context
import gc
gc.collect()
`);
        preparedPython.source.free(); preparedPython.source = null;
        preparedPython.bridge.free(); preparedPython.bridge = null;
        preparedPython.store.free(); preparedPython.store = null;
      });
      assert.equal((await state()).end, 2); assert.equal((await state()).time, 0.5);
      await click(); await step(3000); await step(3250);
      assert.ok(changedPixels(baseline, await capture("source-released-highlight")) > 500);
      await step(3500); assert.equal(changedPixels(baseline, await capture("source-released-restored")), 0);
      await page.evaluate(() => { preparedPython.renderer.seekDirect(2); preparedPython.renderer.render(); });
      assert.ok(changedPixels(halfway, await capture("exact-final-edit")) > 500);
      await page.evaluate(() => { preparedPython.renderer.seekDirect(0.5); preparedPython.renderer.render(); });
      assert.equal(changedPixels(baseline, await capture("final-rewind")), 0);
      result.checks.push("historical-animation-midpoint", "explicit-terminal-same-time-edit", "released-source-still-interactive", "exact-final-rewind");
      assert.deepEqual(await page.evaluate(() => preparedPython.errors), []);
      assert.deepEqual(result.errors, []);
      result.status = "passed";
    } catch (error) {
      result.status = "failed"; result.error = String(error.stack ?? error); throw error;
    } finally { await browser.close(); browser = null; }
  }
  report.status = "passed";
} catch (error) { report.status = "failed"; report.error = String(error.stack ?? error); throw error; }
finally {
  report.resources = cache.stats();
  await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2));
  await browser?.close(); await server?.close();
}
