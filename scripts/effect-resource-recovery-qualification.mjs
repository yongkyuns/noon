// Actual loss/recreation and viewport churn around ordinary authored effects.
// Test hooks observe browser GPU owners; scene meaning stays in the Rust program.
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { disableAuthoringJspi, waitForBrowserObservation } from "./playground-browser-support.mjs";
import { installCaptureState, installWebGpuDeviceCapture } from "./webgpu-device-capture.mjs";

const root = path.resolve(fileURLToPath(new URL("../", import.meta.url)));
const output = path.resolve(root, process.env.NOON_EFFECT_RECOVERY_ARTIFACTS ?? "browser-smoke-artifacts/effect-resource-recovery");
const runtimeBuild = JSON.parse(await readFile(path.join(root, "web/runtime-build-identity.json"), "utf8"));
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
for (const entry of Object.values(runtimeBuild.files)) {
  assert.equal(hash(await readFile(path.join(root, "web", entry.path))), entry.sha256, "runtime package must match its declared identity");
}
const report = {
  sourceRevision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  sourceDirty: execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim() !== "",
  testSourceSha256: hash(await readFile(fileURLToPath(import.meta.url))),
  runtimeBuild, graphicsProfile: "software correctness; no physical performance claim", browsers: [], profiles: [], status: "running",
};
const server = await serveRepository(root, 0);
await mkdir(output, { recursive: true });
const resourceCache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.js"), "utf8"));
const boundaries = [0.5, 2, 2.5, 4, 4.5, 5, 5.5, 6.5, 7, 8];
const workerLessons = [
  { id: "glass", file: "showcase_glass_lens.py", duration: 8.5, time: 6, boundaries,
    factory: "createDirectGlassLensSmokeRenderer" },
  { id: "shader", file: "showcase_shader_material.py", duration: 6, time: 1.5,
    boundaries: [0.5, 2.5, 3, 5, 5.5], factory: "createDirectShaderSweepSmokeRenderer" },
  { id: "bloom", file: "camera_bloom.py", duration: 5, time: 1.5,
    boundaries: [0.5, 2.5, 3], factory: "createDirectCameraBloomRenderer" },
  { id: "glow-100", file: "glow_profile.py", duration: 10, time: 5, boundaries: [],
    sourcePrefix: 'context = {"active_glows": 100, "property": "radius", "duration": 10}\n',
    factory: "createDirectGlowProfileRenderer", factoryArgs: [100, "radius", 10] },
];
report.pythonSources = Object.fromEntries(await Promise.all(workerLessons.map(async lesson =>
  [lesson.file, hash((lesson.sourcePrefix ?? "") + await readFile(path.join(root, "web/python/examples", lesson.file), "utf8"))])));

async function picture(page, name) {
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const bytes = await page.locator("#scene").screenshot({ path: path.join(output, `${name}.png`) });
  return PNG.sync.read(bytes);
}
function exact(actual, expected, label) {
  assert.equal(actual.width, expected.width, label);
  assert.equal(actual.height, expected.height, label);
  assert.deepEqual(actual.data, expected.data, label);
}

function literalPrefixWitness(image) {
  assert.equal(image.width, 320); assert.equal(image.height, 180);
  for (const [x, y, expected] of [[20, 20, [8, 16, 31, 255]], [45, 35, [28, 82, 148, 255]], [65, 35, [240, 166, 70, 255]]]) {
    const at = (y * 320 + x) * 4;
    assert.deepEqual(Array.from(image.data.subarray(at, at + 4)), expected, "literal prefix witnesses reject blank/stale captures");
  }
}

async function direct(context, backend, size = [320, 180], time = 6, factory = "createDirectGlassLensSmokeRenderer", factoryArgs = []) {
  const page = await context.newPage();
  page.setDefaultTimeout(60_000);
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  if (backend === "WebGPU") await installWebGpuDeviceCapture(page);
  await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
  const state = await page.evaluate(async ({ size, time, boundaries, factory, factoryArgs }) => {
    const wasm = await import("./pkg/noon_web.js"); await wasm.default();
    const canvas = document.querySelector("#scene");
    [canvas.width, canvas.height] = size;
    [canvas.style.width, canvas.style.height] = size.map(v => `${v}px`);
    const surface = canvas.transferControlToOffscreen();
    const renderer = await wasm[factory](surface, ...factoryArgs);
    const { sampleDirectProgram, sampleDirectProgramThrough } = await import("../scripts/direct-program-sample.mjs");
    renderer.advanceDirectRealtime(0);
    await sampleDirectProgram(renderer, 0);
    await sampleDirectProgramThrough(renderer, time, boundaries);
    const gpuIdentity = () => {
      if (renderer.rendererBackend() === "WebGPU") {
        const capture = window.__noonWebGpuDeviceCapture;
        const device = capture?.devices[capture.ownerDeviceIndex];
        const info = device?.adapterInfo;
        return { api: "WebGPU", vendor: info?.vendor ?? null, architecture: info?.architecture ?? null,
          device: info?.device ?? null, description: info?.description ?? null };
      }
      const gl = surface.getContext("webgl2");
      return { api: "WebGL2", vendor: gl.getParameter(gl.VENDOR), renderer: gl.getParameter(gl.RENDERER) };
    };
    const snapshot = () => ({ time: renderer.time(), revision: String(renderer.directSceneRevision()),
      objects: renderer.objectCount(), backend: renderer.rendererBackend(), gpuGeneration: renderer.gpuGeneration(),
      frame: JSON.parse(renderer.debugSelectionFrameJson()), gpuIdentity: gpuIdentity() });
    async function present() {
      for (let i = 0; i < 100; i++) {
        if (renderer.render()) return snapshot();
        await new Promise(resolve => setTimeout(resolve, 10));
      }
      throw new Error("direct effect frame did not present");
    }
    window.glass = { renderer, surface, snapshot, present };
    return snapshot();
  }, { size, time, boundaries, factory, factoryArgs });
  assert.equal(state.backend, backend);
  assert.equal(state.time, time);
  return { page, state, errors, async close() {
    await page.evaluate(() => {
      const { renderer, surface } = window.glass;
      const backend = renderer.rendererBackend();
      renderer.retireExecutionSource(); renderer.free();
      if (backend === "WebGL2") surface.getContext("webgl2")?.getExtension("WEBGL_lose_context")?.loseContext();
    });
    await page.close(); assert.deepEqual(errors, []);
  } };
}

async function directProfile(context, backend, lesson) {
  const owner = await direct(context, backend, [320, 180], lesson.time, lesson.factory, lesson.factoryArgs ?? []);
  const { page, state } = owner;
  const result = { host: "direct-rust-wasm", lesson: lesson.id, backend, initial: state, resizes: [] };
  const name = lesson.id === "glass" ? `${backend}-direct` : `${backend}-${lesson.id}-direct`;
  try {
    const initial = await picture(page, `${name}-initial`);
    if (lesson.id === "glass") literalPrefixWitness(initial);
    if (lesson.id === "glow-100") assert.equal(state.frame.objects.flatMap(object => object.effects).length, 100);
    for (const [index, size] of [[640, 360], [240, 160], [320, 180], [640, 360], [320, 180]].entries()) {
      const after = await page.evaluate(async size => {
        const canvas = document.querySelector("#scene");
        [canvas.style.width, canvas.style.height] = size.map(v => `${v}px`);
        window.glass.renderer.resize(...size);
        return window.glass.present();
      }, size);
      assert.equal(after.time, state.time); assert.equal(after.revision, state.revision);
      assert.deepEqual(after.frame, state.frame, "viewport changes retain the exact authored state");
      const warm = await picture(page, `${name}-resize-${index}`);
      const cold = await direct(context, backend, size, lesson.time, lesson.factory, lesson.factoryArgs ?? []);
      try { exact(warm, await picture(cold.page, `${name}-cold-${index}`), "warm resize equals a cold renderer exactly"); }
      finally { await cold.close(); }
      if (size[0] === 320) exact(warm, initial, "return viewport restores initial pixels");
      result.resizes.push({ size, coldExact: true, authoredStateUnchanged: true });
    }
    const hidden = await page.evaluate(() => {
      window.glass.renderer.resize(0, 0);
      return { presented: window.glass.renderer.render(), state: window.glass.snapshot() };
    });
    assert.equal(hidden.presented, false); assert.deepEqual(hidden.state.frame, state.frame);
    await page.evaluate(async () => { window.glass.renderer.resize(320, 180); await window.glass.present(); });
    exact(await picture(page, `${name}-visible-again`), initial, "zero-sized surface preserves authored state and returns exactly");
    if (backend === "WebGPU") {
      await page.evaluate(() => {
        const capture = window.__noonWebGpuDeviceCapture;
        const index = capture.ownerDeviceIndex;
        if (index === null || !capture.devices[index]) throw new Error("glass owner GPUDevice was not captured");
        window.glass.lostIndex = index; capture.devices[index].destroy();
      });
      await page.waitForFunction(() => window.__noonWebGpuDeviceCapture.lost[window.glass.lostIndex] !== null);
    } else {
      await page.evaluate(() => {
        const { surface } = window.glass;
        const extension = surface.getContext("webgl2")?.getExtension("WEBGL_lose_context");
        if (!extension) throw new Error("real WebGL context loss is unavailable");
        window.glass.loss = { lost: 0, restored: 0, extension };
        surface.addEventListener("webglcontextlost", event => { event.preventDefault(); window.glass.loss.lost++; });
        surface.addEventListener("webglcontextrestored", () => window.glass.loss.restored++);
        extension.loseContext();
      });
      await page.waitForFunction(() => window.glass.loss.lost === 1);
      await page.evaluate(() => window.glass.loss.extension.restoreContext());
      await page.waitForFunction(() => window.glass.loss.restored === 1);
    }
    const recovered = await page.evaluate(async backend => {
      const { renderer } = window.glass;
      for (let i = 0; i < 100; i++) {
        const restored = backend === "WebGPU" ? await renderer.recoverWebGpuDevice() : await renderer.recoverWebGlContext();
        if (restored) return window.glass.present();
        await new Promise(resolve => setTimeout(resolve, 10));
      }
      throw new Error("glass GPU owner did not recover");
    }, backend);
    assert.equal(recovered.gpuGeneration, state.gpuGeneration + 1);
    assert.equal(recovered.revision, state.revision); assert.equal(recovered.time, state.time);
    assert.deepEqual(recovered.frame, state.frame, "GPU loss cannot restart or advance the authored continuation");
    exact(await picture(page, `${name}-recovered`), initial, "recovered effect frame is exact");
    result.loss = { gpuGeneration: recovered.gpuGeneration, pixelsExact: true, authoredStateUnchanged: true };
    result.zeroSize = { presented: false, restoredExact: true };
    return result;
  } finally { await owner.close(); }
}

// Observe the real browser surface in each render worker before initialization.
// Hooks forward every operation unchanged; only loss stimulus is injected later.
function captureWorkerSurface() {
  const original = OffscreenCanvas.prototype.getContext;
  const seen = new WeakSet();
  const state = { canvas: null, device: null, gl: null, lost: 0, restored: 0 };
  globalThis.__noonGlassSurface = state;
  OffscreenCanvas.prototype.getContext = function(type, ...args) {
    const context = original.call(this, type, ...args);
    if (context === null || seen.has(context)) return context;
    seen.add(context); state.canvas = this;
    if (type === "webgpu") {
      const configure = context.configure;
      Object.defineProperty(context, "configure", { configurable: true, value: function(descriptor) {
        const result = configure.call(this, descriptor); state.device = descriptor.device; return result;
      } });
    } else if (type === "webgl2") {
      state.gl = context;
      this.addEventListener("webglcontextlost", event => { event.preventDefault(); state.lost++; });
      this.addEventListener("webglcontextrestored", () => state.restored++);
    }
    return context;
  };
}
async function instrumentRenderWorker(context) {
  await context.route("**/authoring-render-worker.js", async route => {
    const response = await route.fetch();
    const prefix = `(${installCaptureState.toString()})();\n(${captureWorkerSurface.toString()})();\n`;
    await route.fulfill({ response, body: prefix + await response.text() });
  });
}
async function waitWorker(worker, predicate, label) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    if (await worker.evaluate(predicate)) return;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  throw new Error(label);
}
async function pythonProfile(context, backend, lesson) {
  const page = await context.newPage(); page.setDefaultTimeout(60_000);
  const errors = []; page.on("pageerror", error => errors.push(String(error)));
  const source = (lesson.sourcePrefix ?? "") + await readFile(path.join(root, "web/python/examples", lesson.file), "utf8");
  await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
  const state = await page.evaluate(async ({ source, boundaries, duration, time }) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const canvas = document.querySelector("#scene");
    canvas.width = 320; canvas.height = 180; canvas.style.width = "320px"; canvas.style.height = "180px";
    const authoring = new PythonAuthoringClient(); const failures = [];
    const execution = new AuthoringExecutionClient(canvas, { onError: error => failures.push(String(error)) });
    let resolveAttached, rejectAttached, registration = null;
    const attached = new Promise((resolve, reject) => { resolveAttached = resolve; rejectAttached = reject; });
    const completed = authoring.run(source, {}, { onSemanticContinuation: async next => {
      try {
        if (registration !== null) throw new Error("glass source registered twice");
        registration = next.semanticExecution;
        await execution.startSemanticExecution(registration, {
          authoringClient: authoring, loopDurationSeconds: duration, transportMode: "transferable", pacing: "external_samples",
        }); resolveAttached();
      } catch (error) { rejectAttached(error); throw error; }
    } });
    void completed.then(() => { if (registration === null) rejectAttached(new Error("glass source did not suspend")); }, rejectAttached);
    await attached;
    for (const sample of [0, ...boundaries, duration]) await execution.sampleToAuthoredTime(sample);
    const result = await completed;
    if (result.duration !== duration || result.semanticExecution.contextId !== registration.contextId) throw new Error("effect source completion changed its context");
    await execution.reconcileSemanticExecution({
      contextId: result.semanticExecution.contextId, callbackSessionId: result.semanticExecution.callbackSessionId,
      continuationGeneration: null,
    }, { authoringClient: authoring, loopDurationSeconds: duration });
    await execution.pause();
    await execution.seek(time);
    const snapshot = async () => ({ frame: await execution.debugFrame(), metrics: (await execution.metrics()).metrics });
    window.glass = { authoring, execution, snapshot, failures, registration, sourceCompleted: true };
    return snapshot();
  }, { source, ...lesson });
  const result = { host: "pyodide-worker", lesson: lesson.id, backend, initial: state, resizes: [] };
  try {
    assert.equal(state.metrics.backend, backend); assert.equal(state.metrics.time, lesson.time);
    assert.equal(state.frame.time, lesson.time);
    const initial = await picture(page, `${backend}-${lesson.id}-python-initial`);
    if (lesson.id === "glass") literalPrefixWitness(initial);
    if (lesson.id === "shader") assert.equal(state.frame.objects.flatMap(object => object.effects).length, 2);
    if (lesson.id === "glow-100") assert.equal(state.frame.objects.flatMap(object => object.effects).length, 100);
    const coldInitial = await direct(context, backend, [320, 180], lesson.time, lesson.factory, lesson.factoryArgs ?? []);
    try { exact(initial, await picture(coldInitial.page, `${backend}-${lesson.id}-python-direct-oracle`), "completed Python seek equals direct Rust forward execution"); }
    finally { await coldInitial.close(); }
    for (const [index, size] of [[640, 360], [240, 160], [320, 180]].entries()) {
      const after = await page.evaluate(async ({size, time}) => {
        const canvas = document.querySelector("#scene");
        [canvas.style.width, canvas.style.height] = size.map(v => `${v}px`);
        window.glass.execution.resize(...size, 1);
        await window.glass.execution.advanceToWithRendererObservation(time);
        return window.glass.snapshot();
      }, {size, time: lesson.time});
      assert.equal(after.metrics.time, lesson.time); assert.deepEqual(after.frame, state.frame);
      const warm = await picture(page, `${backend}-${lesson.id}-python-resize-${index}`);
      const cold = await direct(context, backend, size, lesson.time, lesson.factory, lesson.factoryArgs ?? []);
      try { exact(warm, await picture(cold.page, `${backend}-${lesson.id}-python-cold-${index}`), "resized Python replay equals cold direct Rust exactly"); }
      finally { await cold.close(); }
      if (size[0] === 320) exact(warm, initial, "Python return viewport is exact");
      result.resizes.push({ size, coldDirectExact: true, authoredStateUnchanged: true });
    }
    const worker = page.workers().find(value => value.url().includes("execution-render-worker.js") && !value.url().includes("capability-probe"));
    assert.ok(worker, "qualification requires the actual render worker");
    if (backend === "WebGL2") {
      await worker.evaluate(() => {
        const state = globalThis.__noonGlassSurface;
        const extension = state?.gl?.getExtension("WEBGL_lose_context");
        if (!extension) throw new Error("worker context loss is unavailable");
        state.extension = extension; extension.loseContext();
      });
      await waitWorker(worker, () => globalThis.__noonGlassSurface.lost === 1, "render worker did not observe real context loss");
      const during = await page.evaluate(() => window.glass.snapshot());
      assert.deepEqual(during.frame, state.frame);
      await worker.evaluate(() => globalThis.__noonGlassSurface.extension.restoreContext());
      await waitWorker(worker, () => globalThis.__noonGlassSurface.restored === 1, "render worker did not observe restoration");
      await waitForBrowserObservation(page, async generation => {
        const metrics = (await window.glass.execution.metrics()).metrics;
        return metrics.gpuGeneration === generation + 1 && metrics.presentedFrames > 0 && !metrics.needsPresent;
      }, state.metrics.gpuGeneration);
    } else {
      await worker.evaluate(() => {
        const device = globalThis.__noonGlassSurface?.device;
        if (!device) throw new Error("render worker's actual GPUDevice was not captured");
        globalThis.__noonGlassLost = null;
        device.lost.then(info => { globalThis.__noonGlassLost = { reason: String(info.reason), message: String(info.message) }; }); device.destroy();
      });
      await waitWorker(worker, () => globalThis.__noonGlassLost !== null, "actual worker GPUDevice did not become lost");
      await page.evaluate(async () => {
        try { await window.glass.execution.metrics(); } catch (error) { window.glass.lossProbe = String(error); }
      });
      const beforeRestart = await page.evaluate(() => ({ failures: window.glass.failures, probe: window.glass.lossProbe }));
      const browserLoss = await worker.evaluate(() => globalThis.__noonGlassLost);
      assert.equal(browserLoss.reason, "destroyed");
      await page.evaluate(() => window.glass.execution.restart());
      result.lossDiagnostic = { mode: "explicit worker restart", browserLoss, ...beforeRestart };
    }
    const recovered = await page.evaluate(() => window.glass.snapshot());
    assert.deepEqual(recovered.frame, state.frame, "recovery preserves the authored replay time without a repair seek");
    assert.equal(recovered.metrics.time, lesson.time);
    exact(await picture(page, `${backend}-${lesson.id}-python-recovered`), initial, "Python effect recovery is exact");
    if (backend === "WebGL2") {
      assert.equal(recovered.metrics.gpuGeneration, state.metrics.gpuGeneration + 1);
      assert.deepEqual(await page.evaluate(() => window.glass.failures), []);
    }
    const beforeRecreation = await page.evaluate(() => window.glass.snapshot());
    await page.evaluate(() => window.glass.execution.restart());
    const recreated = await page.evaluate(() => window.glass.snapshot());
    assert.deepEqual(recreated.frame, beforeRecreation.frame, "explicit worker recreation retains current shared scene state");
    exact(await picture(page, `${backend}-${lesson.id}-python-recreated`), initial, "recreated worker effect frame is exact");
    assert.ok(!page.workers().includes(worker), "recreation must retire the old render worker");
    assert.equal(page.workers().filter(value => value.url().includes("execution-render-worker.js") && !value.url().includes("capability-probe")).length, 1, "only the replacement render worker remains alive");
    const idleBefore = await page.evaluate(() => window.glass.snapshot());
    await page.waitForTimeout(150);
    const idleAfter = await page.evaluate(() => window.glass.snapshot());
    assert.deepEqual(idleAfter.frame, idleBefore.frame);
    assert.equal(idleAfter.metrics.presentedFrames, idleBefore.metrics.presentedFrames, "paused effect must sleep");
    assert.equal(idleAfter.metrics.needsPresent, false);
    assert.equal(idleAfter.metrics.bufferedDeltas, 0);
    result.idle = { observationMs: 150, presentationsUnchanged: true, authoredStateUnchanged: true, pendingWork: false };
    result.loss = { pixelsExact: true, authoredStateUnchanged: true, recovered: recovered.metrics };
    result.recreation = { pixelsExact: true, authoredStateUnchanged: true, metrics: recreated.metrics };
    assert.deepEqual(errors, []);
    return result;
  } finally {
    await page.evaluate(() => { window.glass.execution.terminate(); window.glass.authoring.terminate(); });
    await page.close();
  }
}

try {
  for (const backend of ["WebGPU", "WebGL2"]) {
    const browser = await playwright.chromium.launch({ channel: "chromium", headless: true,
      args: browserArgs(backend === "WebGPU" ? "webgpu" : "webgl") });
    report.browsers.push({ backend, version: browser.version() });
    try {
      const context = await browser.newContext({ viewport: { width: 1000, height: 700 } });
      await resourceCache.install(context); await disableAuthoringJspi(context); await instrumentRenderWorker(context);
      for (const lesson of workerLessons.filter(lesson => ["glass", "glow-100"].includes(lesson.id))) {
        report.profiles.push(await directProfile(context, backend, lesson));
        console.log(`[PASS] ${backend}/${lesson.id}: direct resize, zero surface and real GPU loss recovery`);
      }
      for (const lesson of workerLessons) {
        report.profiles.push(await pythonProfile(context, backend, lesson));
        console.log(`[PASS] ${backend}/${lesson.id}: Python seek, resize, real loss and worker recreation`);
      }
    } finally { await browser.close(); }
  }
  assert.equal(report.profiles.length, 12);
  report.status = "passed";
} catch (error) { report.status = "failed"; report.error = String(error.stack ?? error); throw error; }
finally {
  report.pyodideResourceCache = resourceCache.stats();
  await writeFile(path.join(output, "report.json"), `${JSON.stringify(report, (_key, value) => typeof value === "bigint" ? value.toString() : value, 2)}\n`);
  await server.close();
}
