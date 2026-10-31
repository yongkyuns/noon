// Ordinary public Scene.construct uses the existing portable source compiler and
// continuation awaitable. Rust owns completion, history and look-ahead demand.
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { createBrowserFailureTrace } from "./playground-browser-failure-evidence.mjs";
import { PYTHON_COMPAT_MODULES } from "../web/python-compat-modules.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_PREPARED_CONSTRUCT_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/prepared-canvas/construct");
const worker = await readFile(path.join(root, "web/python-worker.source.js"), "utf8");
const pyodideUrl = worker.match(/import \{ loadPyodide \} from "([^"]+)";/)?.[1];
assert.ok(pyodideUrl);
const modules = await Promise.all(PYTHON_COMPAT_MODULES.map(async ({sourcePath, runtimePath}) => ({
  runtimePath, source: await readFile(path.join(root, "web", sourcePath), "utf8"),
})));
const cache = createPyodideResourceCache(worker);
const report = { status: "running", cases: [] };
await mkdir(output, { recursive: true });
let server, browser;
function changedPixels(a, b) {
  assert.equal(a.width, b.width); assert.equal(a.height, b.height);
  let changed = 0;
  for (let i = 0; i < a.data.length; i += 4) if (!a.data.subarray(i, i + 4).equals(b.data.subarray(i, i + 4))) changed++;
  return changed;
}
const source = `
from noon import Scene, Circle, Indicate, BLUE, RIGHT, linear
from js import noonPreparedObservation

class OrdinaryPrepared(Scene):
    def construct(self):
        circle = Circle(radius=0.8, color=BLUE, fill_opacity=1.0, stroke_width=0.0)
        self.add(circle)
        self.on_click(circle, Indicate(circle, run_time=0.8))
        try:
            self.wait(1.0)
            noonPreparedObservation("completed", 0, circle.get_center().x)
            for i in range(1, 13):
                self.play(circle.animate.shift(RIGHT * 0.25), run_time=0.25, rate_func=linear)
                assert abs(circle.get_center().x - i * 0.25) < 1e-6
                noonPreparedObservation("completed", i, circle.get_center().x)
                if mode == "error" and i == 2:
                    raise RuntimeError("intentional prepared construct failure")
            circle.shift(RIGHT * 0.5)
        finally:
            noonPreparedObservation("finally", 0, 0)

    def tear_down(self):
        noonPreparedObservation("tear-down", 0, 0)
`;

try {
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await chromium.launch({ headless: true, args: browserArgs(backend), channel: "chromium" });
    for (const scenario of ["bounded", "error", "cancel"]) {
      const result = { backend, scenario, status: "running", checks: [], errors: [], console: [] };
      report.cases.push(result);
      const context = await browser.newContext({ viewport: { width: 900, height: 600 } });
      await cache.install(context);
      const page = await context.newPage(); page.setDefaultTimeout(180_000);
      const failureTrace = createBrowserFailureTrace();
      const detachFailureTrace = failureTrace.attach(browser, context, page);
      page.on("pageerror", error => result.errors.push(String(error)));
      page.on("console", message => result.console.push(message.text()));
      const state = () => page.evaluate(() => ({
        phase: constructHost.driver.state(), events: constructHost.events.slice(), prepared: constructHost.prepared,
        outcome: constructHost.outcome, frames: constructHost.direct?.stats().presentedFrames,
        playback: constructHost.renderer ? JSON.parse(constructHost.renderer.debugPreparedPlaybackJson()) : null,
      }));
      const capture = async label => {
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        const bytes = await page.locator("#scene").screenshot();
        await writeFile(path.join(output, `${backend}-${scenario}-${label}.png`), bytes);
        return PNG.sync.read(bytes);
      };
      const click = async () => {
        const rect = await page.locator("#scene").boundingBox();
        await page.mouse.click(rect.x + 320, rect.y + 180);
      };
      try {
        failureTrace.setPhase("navigation");
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
        failureTrace.setPhase("source-bootstrap");
        await page.evaluate(async ({modules, pyodideUrl, source, scenario}) => {
          globalThis.Worker = class { constructor() { throw new Error("same-context construct attempted worker transport"); } };
          const wasm = await import("./pkg/noon_web.js"); await wasm.default();
          const { loadPyodide } = await import(pyodideUrl);
          const pyodide = await loadPyodide();
          const store = new wasm.WasmAuthoringStore();
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
          const { createPreparedInterpreterDriver } = await import("./prepared-interpreter-driver.js");
          const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
          const { attachNativeInputs } = await import("./native-inputs.js");
          const host = { pyodide, store, events: [], prepared: 0, outcome: null, errors: [], renderer: null, source: null, bridge: null, direct: null };
          globalThis.constructHost = host;
          globalThis.noonPreparedObservation = (kind, index, x) => host.events.push({ kind, index, x });
          const ahead = scenario === "error" ? 100 : 0.75;
          host.driver = createPreparedInterpreterDriver({
            async prepareSegment(context, signal) {
              signal.throwIfAborted();
              host.source = context;
              // Canonical Rust completes its actual pending receipt. No Python
              // wrapper or JS test guesses an end time or drives individual frames.
              context.livePrepareSegment();
              if (host.bridge === null) {
                host.bridge = new wasm.InterpreterCanvasFixture(context);
                const canvas = document.querySelector("#scene");
                const renderer = await host.bridge.createCanvas(canvas.transferControlToOffscreen());
                if (signal.aborted) { renderer.free(); signal.throwIfAborted(); }
                host.renderer = renderer; renderer.setPreparedPlaying(false);
                host.detach = attachNativeInputs(renderer, canvas, { inspectionZoom: true,
                  onError: error => host.errors.push(String(error)), onInput: () => host.direct?.wake() });
                host.direct = createDirectExecutionWakeDriver(renderer, { onDemand: () => host.driver.wake() });
              } else {
                host.bridge.publishCompleted(context, host.renderer, false);
                host.direct.wake();
              }
              host.prepared++;
            },
            needsPreparation: () => host.bridge.needsPreparation(host.renderer, ahead),
            finishSource(_, signal) {
              signal.throwIfAborted();
              if (host.bridge === null) throw new Error("source produced no readable interval");
              host.bridge.publishCompleted(host.source, host.renderer, true);
              host.direct.wake();
            },
          });
          globalThis.noonRequireSemanticContinuationActive = () => {
            if (host.driver.state() !== "running") throw new Error("retired prepared source invocation");
          };
          globalThis.noonAwaitSemanticContinuation = context => host.driver.awaitSegment(context);
          // Required Python callbacks are not admitted in this prepared slice.
          globalThis.noonSetSemanticContinuationCallbackSession = () => { throw new Error("prepared source callbacks unsupported"); };
          pyodide.globals.set("construct_source", source); pyodide.globals.set("construct_mode", scenario);
          host.done = host.driver.run(() => pyodide.runPythonAsync(`
import sys
sys.path.insert(0, "/tmp")
from _manim_source_execution import compile_authoring_source, BARRIER_GLOBAL, MODULE_BARRIER_GLOBAL, authoring_source_scope, execute_authoring_module
from _manim_scene import execute_construct, await_source_barrier, await_module_source_barrier
code, pairs = compile_authoring_source(construct_source, filename="prepared-ordinary.py")
assert pairs, "ordinary construct must use the existing portable compiler"
namespace = {"__name__": "__main__", "mode": construct_mode, BARRIER_GLOBAL: await_source_barrier, MODULE_BARRIER_GLOBAL: await_module_source_barrier}
with authoring_source_scope():
    await execute_authoring_module(code, namespace)
scene = namespace["OrdinaryPrepared"]()
await execute_construct(scene, portable_constructs=pairs)
`)).then(() => { host.outcome = "fulfilled"; }, error => { host.outcome = "rejected"; host.failure = String(error); });
        }, {modules, pyodideUrl, source, scenario});
        failureTrace.setPhase("waiting-first-prefix");
        await page.waitForFunction(() => constructHost.driver.state() === "waiting" || constructHost.outcome !== null);
        failureTrace.setPhase("initial-history");
        let start = await state();
        assert.ok(start.playback, JSON.stringify(start));
        assert.equal(await page.evaluate(() => constructHost.renderer.rendererBackend()), backend === "webgpu" ? "WebGPU" : "WebGL2");
        assert.equal(start.playback.time, 0.5); assert.equal(start.playback.end, null);
        if (scenario !== "error") {
          assert.equal(start.phase, "waiting"); assert.equal(start.outcome, null);
          assert.equal(start.prepared, 2); assert.equal(start.playback.frontier, 1.25);
          assert.deepEqual(start.events, [{ kind: "completed", index: 0, x: 0 }]);
          await page.waitForTimeout(160);
          const settled = await state();
          assert.equal(settled.prepared, start.prepared); assert.equal(settled.frames, start.frames);
          assert.deepEqual(settled.events, start.events);
          result.checks.push("ordinary-construct-suspended", "rust-demand-caps-history", "no-idle-task-or-frame-loop");
        }
        failureTrace.setPhase("history-interaction");
        const baseline = await capture("baseline");
        await click();
        await page.waitForFunction(() => JSON.parse(constructHost.renderer.debugPreparedPlaybackJson()).interactions);
        await page.waitForTimeout(100);
        result.highlightPixels = changedPixels(baseline, await capture("highlight"));
        assert.ok(result.highlightPixels > 500);
        await page.waitForFunction(() => !JSON.parse(constructHost.renderer.debugPreparedPlaybackJson()).interactions && constructHost.direct.stats().idle);
        assert.equal(changedPixels(baseline, await capture("restored")), 0);
        result.checks.push("click-while-interpreter-suspended-or-failed", "exact-restoration");
        failureTrace.setPhase(`source-${scenario}`);
        if (scenario === "bounded") {
          await page.evaluate(() => { constructHost.renderer.seekDirect(0.75); constructHost.direct.wake(); });
          await page.waitForFunction(() => constructHost.driver.state() === "waiting" && constructHost.prepared === 3);
          const advanced = await state();
          assert.equal(advanced.playback.frontier, 1.5); assert.equal(advanced.playback.time, 0.75);
          assert.equal(advanced.events.at(-1).x, 0.25, "next source read sees ordinary semantic completion");
          await page.evaluate(() => { constructHost.renderer.seekDirect(0.5); constructHost.direct.wake(); });
          assert.equal(changedPixels(baseline, await capture("paused-rewind")), 0);
          assert.equal((await state()).prepared, 3);
          await page.evaluate(() => { constructHost.renderer.setPreparedPlaying(true); constructHost.direct.wake(); });
          await page.waitForFunction(() => {
            const p = JSON.parse(constructHost.renderer.debugPreparedPlaybackJson());
            return p.time > 1.5 && p.end === null && constructHost.prepared > 3;
          });
          result.duringPlayback = await state();
          assert.ok(changedPixels(baseline, await capture("playing-during-preparation")) > 500);
          await page.waitForFunction(() => constructHost.outcome === "fulfilled" &&
            JSON.parse(constructHost.renderer.debugPreparedPlaybackJson()).time === 4 && constructHost.direct.stats().idle);
          const final = await state();
          assert.equal(final.phase, "finished"); assert.equal(final.playback.end, 4); assert.equal(final.prepared, 13);
          assert.equal(final.events.filter(e => e.kind === "completed").length, 13);
          assert.equal(final.events.filter(e => e.kind === "finally").length, 1);
          assert.equal(final.events.filter(e => e.kind === "tear-down").length, 1);
          result.checks.push("navigation-resumes-source", "completion-visible-to-python", "moving-playback-with-preparation", "exact-terminal-no-reexecution");
        } else if (scenario === "cancel") {
          await page.evaluate(() => constructHost.driver.cancel());
          await page.waitForFunction(() => constructHost.outcome !== null);
          const stopped = await state();
          assert.equal(stopped.phase, "cancelled"); assert.equal(stopped.outcome, "rejected");
          assert.equal(stopped.prepared, 2); assert.equal(stopped.playback.end, null);
          assert.equal(stopped.events.filter(e => e.kind === "finally").length, 1);
          assert.equal(stopped.events.filter(e => e.kind === "tear-down").length, 1);
          result.checks.push("cancel-unwinds-interpreter", "no-false-terminal-or-retry");
        } else {
          const stopped = await state();
          assert.equal(stopped.phase, "failed"); assert.equal(stopped.outcome, "rejected");
          assert.equal(stopped.prepared, 3); assert.equal(stopped.playback.end, null);
          assert.match(await page.evaluate(() => constructHost.failure), /intentional prepared construct failure/);
          assert.equal(stopped.events.filter(e => e.kind === "finally").length, 1);
          assert.equal(stopped.events.filter(e => e.kind === "tear-down").length, 1);
          result.checks.push("source-error-not-retried", "no-false-terminal");
        }
        failureTrace.setPhase("released-source-history");
        await page.evaluate(async () => {
          await constructHost.done;
          await constructHost.pyodide.runPythonAsync(`del scene, namespace\nimport gc\ngc.collect()`);
          constructHost.source.free(); constructHost.source = null;
          constructHost.bridge.free(); constructHost.bridge = null;
          constructHost.store.free(); constructHost.store = null;
          constructHost.renderer.setPreparedPlaying(false);
          constructHost.renderer.seekDirect(0.5); constructHost.direct.wake();
        });
        assert.equal(changedPixels(baseline, await capture("released-source-rewind")), 0);
        await click();
        await page.waitForFunction(() => JSON.parse(constructHost.renderer.debugPreparedPlaybackJson()).interactions);
        await page.waitForFunction(() => !JSON.parse(constructHost.renderer.debugPreparedPlaybackJson()).interactions && constructHost.direct.stats().idle);
        assert.equal(changedPixels(baseline, await capture("released-source-restored")), 0);
        result.checks.push("released-source-history-remains-interactive");
        assert.deepEqual(await page.evaluate(() => constructHost.errors), []);
        assert.deepEqual(result.errors, []);
        result.final = await state(); result.status = "passed";
      } catch (error) {
        result.status = "failed"; result.error = String(error.stack ?? error);
        // Capture host-side evidence before querying a potentially dead page.
        // This snapshot excludes deliberate context/browser cleanup below.
        result.failureTrace = failureTrace.snapshot();
        try { result.state = await state(); result.failure = await page.evaluate(() => constructHost.failure); }
        catch (stateError) { result.stateCaptureError = String(stateError).slice(0, 2000); }
        throw error;
      } finally {
        try { await context.close(); }
        finally { detachFailureTrace(); }
      }
    }
    await browser.close(); browser = null;
  }
  report.status = "passed";
} catch (error) { report.status = "failed"; report.error = String(error.stack ?? error); throw error; }
finally {
  report.resources = cache.stats();
  await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2));
  await browser?.close(); await server?.close();
}
