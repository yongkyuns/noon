// Exercise the real gallery/main.js, source restart router, controls, production
// Python worker and renderer. Only input source/manifest and the existing run
// options are selected by the harness; no runtime, clock or handoff is replaced.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(root, "browser-smoke-artifacts/prepared-gallery");
const source = `from noon import *
events = []
class PreparedGallery(Scene):
    def construct(self):
        events.append("start")
        target = Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0)
        self.add(target)
        self.wait(0)
        self.wait(30)
        assert events == ["start"]
        events.append("first")
        self.play(target.animate.shift(2 * RIGHT), run_time=1, rate_func=linear)
        assert abs(target.get_center()[0] - 2) < 1e-6
        assert events == ["start", "first"]
        events.append("moved")
        self.wait(30)
        assert events == ["start", "first", "moved"]
        target.shift(RIGHT)
`;
const failureSource = source.slice(0, source.indexOf("        self.play(")) +
  '        raise RuntimeError("intentional gallery source failure")\n';
const staticSource = `from noon import *
from js import console
class StaticPrepared(Scene):
    def construct(self):
        console.log("noon-static-source-invoked")
        self.add(Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0))
`;
const zeroSource = `from noon import *
from js import console
class ZeroPrepared(Scene):
    def construct(self):
        console.log("noon-zero-source-invoked")
        target = Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0)
        target.shift(RIGHT)
        self.add(target)
        self.wait(0)
        self.wait(0)
        target.shift(LEFT)
`;
const entries = ["prepared-lifecycle", "prepared-failure", "prepared-static", "prepared-zero"].map((id, order) => ({
  id, order, title: id, status: "ready", parity_status: "candidate",
  reuse: "manim-compatible-parity-v0.21", category: "qualification",
  path: `python/examples/${id}.py`, thumbnail: "prepared-fixture.svg",
  features: ["prepared-lifecycle"], summary: "Bounded lifecycle qualification fixture",
}));
const hash = value => createHash("sha256").update(value).digest("hex");
const report = { status: "running", scope: "actual gallery lifecycle; bounded fixture, not gallery compatibility or performance",
  sourceSha256: hash(source), staticSourceSha256: hash(staticSource), zeroSourceSha256: hash(zeroSource), captureBackdrop: "opaque canvas-wrap; CSS corner/shadow decoration disabled; unchanged full-canvas RGBA equality", cases: [] };
const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.source.js"), "utf8"));
let browser, server;

async function installFixture(page) {
  await cache.install(page.context());
  await page.route("**/python/examples/manim_*_manifest.json", route => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ reference: { version: "0.21" },
      entries: route.request().url().endsWith("manim_tutorial_manifest.json") ? entries : [] }),
  }));
  for (const entry of entries) {
    await page.route(`**/${entry.path}`, route => route.fulfill({ contentType: "text/x-python",
      body: entry.id === "prepared-zero" ? zeroSource : entry.id === "prepared-static" ? staticSource : entry.id === "prepared-failure" ? failureSource : source }));
  }
  await page.route("**/prepared-fixture.svg", route => route.fulfill({ contentType: "image/svg+xml",
    body: '<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90"><circle cx="80" cy="45" r="20" fill="blue"/></svg>' }));
  await page.addInitScript(() => {
    globalThis.__NOON_PLAYGROUND_TEST_HOOKS__ = {
      authoringRunOptions: () => ({ preparedPlayback: true, preparationLookaheadSeconds: 0.75 }),
    };
    // Count native worker construction without intercepting messages or work.
    globalThis.galleryWorkers = [];
    const NativeWorker = globalThis.Worker;
    globalThis.Worker = class extends NativeWorker {
      constructor(url, options) { super(url, options); galleryWorkers.push(options?.name ?? ""); }
    };
  });
}

async function openReady(page, frontier = 30) {
  await page.waitForFunction(expected => {
    const c = document.querySelector(".playback-controls"), seek = document.querySelector(".playback-prepared-seek");
    return c?.dataset.busy === "false" && c.dataset.durationKnown === "false" &&
      seek && !seek.hidden && !seek.disabled && Number(seek.max) === expected;
  }, frontier);
}
async function pause(page) {
  if (await page.locator(".playback-controls").getAttribute("data-playing") === "true") {
    await page.locator(".playback-toggle").click();
  }
  await page.waitForFunction(() => {
    const c = document.querySelector(".playback-controls");
    return c?.dataset.playing === "false" && c.dataset.busy === "false";
  });
}
async function seekOpen(page, time, frontier = null) {
  const field = page.locator(".playback-prepared-seek");
  await field.fill(String(time)); await field.dispatchEvent("change");
  await page.waitForFunction(t => {
    const c = document.querySelector(".playback-controls");
    return c?.dataset.busy === "false" && Number(c.dataset.elapsedSeconds) === t;
  }, time);
  if (frontier !== null) await openReady(page, frontier);
}
async function seekSealed(page, time) {
  await page.locator(".playback-scrubber").evaluate((element, time) => {
    element.value = String(time); element.dispatchEvent(new Event("input", { bubbles: true }));
  }, time);
  await page.waitForFunction(t => {
    const c = document.querySelector(".playback-controls");
    return c?.dataset.busy === "false" && Number(c.dataset.elapsedSeconds) === t;
  }, time);
}
async function assertGallerySourceFailure(page) {
  // The status is the exception title. Its message belongs to the visible
  // diagnostic panel, not the status or the echoed Python source excerpt.
  const panel = page.locator("#authoring-problem");
  await panel.waitFor({ state: "visible" });
  assert.equal(await page.locator("#patch-status").evaluate(element => element.value ?? element.textContent), "Python raised RuntimeError");
  assert.equal(await panel.locator("strong").textContent(), "Python raised RuntimeError");
  assert.equal(await panel.locator("p").first().textContent(), "intentional gallery source failure");
}
async function selectGalleryExample(page, id) {
  // The production picker keeps cards hidden until its trigger opens the dialog.
  // Use ordinary DOM clicks; do not force a hidden click or bypass selection.
  const layer = page.locator(".example-browser-layer");
  await page.locator("#example-browser-trigger").click();
  await layer.waitFor({ state: "visible" });
  await layer.locator(`.example-card[data-example-id="${id}"]`).click();
  await layer.waitFor({ state: "hidden" });
}
async function pixels(page, name) {
  await page.waitForFunction(async () => {
    const metrics = (await __noonExampleGallery.executionMetrics())?.metrics;
    return metrics?.ready && !metrics.needsPresent && metrics.bufferedDeltas === 0;
  });
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const bytes = await page.locator("#scene").screenshot();
  await writeFile(path.join(output, `${name}.png`), bytes);
  return PNG.sync.read(bytes).data;
}

await mkdir(output, { recursive: true });
try {
  report.mainSha256 = hash(await readFile(path.join(root, "web/main.js")));
  report.workerSha256 = hash(await readFile(path.join(root, "web/python-worker.js")));
  report.wasmSha256 = hash(await readFile(path.join(root, "web/pkg/noon_web_bg.wasm")));
  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch({ headless: true, channel: "chromium", args: browserArgs(backend),
      ...(process.env.NOON_BROWSER_PROXY ? { proxy: { server: process.env.NOON_BROWSER_PROXY, bypass: "127.0.0.1,localhost" } } : {}) });
    for (const transport of ["transferable", "shared"]) {
      const entry = { backend, transport, status: "running", checks: [] }; report.cases.push(entry);
      server = await serveRepository(root, 0, { crossOriginIsolated: transport === "shared" });
      const page = await browser.newPage({ viewport: { width: 1100, height: 900 }, deviceScaleFactor: 1 });
      page.setDefaultTimeout(180_000);
      const pageErrors = []; page.on("pageerror", error => pageErrors.push(String(error)));
      let staticInvocations = 0, zeroInvocations = 0;
      page.on("console", message => { if (message.text() === "noon-static-source-invoked") staticInvocations++;
        if (message.text() === "noon-zero-source-invoked") zeroInvocations++; });
      try {
        await installFixture(page);
        await page.goto(`${server.baseUrl}/web/index.html?example=prepared-lifecycle`);
        await page.waitForFunction(() => globalThis.__noonExampleGallery !== undefined);
        // CSS corner/shadow compositing can vary by one byte across captures.
        // Remove only that decoration and make its backdrop opaque. Keep canvas
        // dimensions, renderer, full screenshot and exact RGBA assertions.
        await page.addStyleTag({ content: ".canvas-wrap { background: #090c13 !important; } #scene { border-radius: 0 !important; box-shadow: none !important; }" });
        await page.evaluate(() => {
          if (!__noonExampleGallery.runInFlight && !document.querySelector(".playback-controls")) void __noonExampleGallery.run();
        });
        await openReady(page); await pause(page);
        assert.equal(await page.evaluate(() => __noonExampleGallery.runInFlight), true);
        assert.equal(await page.locator(".gallery-controls input").isDisabled(), false);
        assert.equal(await page.locator('.example-card[data-example-id="prepared-failure"]').isDisabled(), false);
        const metrics = await page.evaluate(async () => (await __noonExampleGallery.executionMetrics()).metrics);
        assert.equal(metrics.backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
        assert.equal(await page.evaluate(() => __noonExampleGallery.transportMode), transport);
        entry.checks.push("actual-gallery-prepared-start", "source-does-not-lock-controls-or-gallery");
        await seekOpen(page, 5);
        const baseline = await pixels(page, `${backend}-${transport}-baseline`);
        await page.evaluate(() => { globalThis.originalGalleryCanvas = document.querySelector("#scene"); });
        await seekOpen(page, 29.5, 31); await seekOpen(page, 30.5, 61); await seekOpen(page, 60.5);
        await page.waitForFunction(() => !__noonExampleGallery.runInFlight &&
          document.querySelector(".playback-controls")?.dataset.durationKnown === "true");
        assert.equal(await page.locator(".playback-scrubber").getAttribute("max"), "61");
        assert.equal(await page.locator(".playback-controls").getAttribute("data-playing"), "false");
        assert.equal(await page.evaluate(() => originalGalleryCanvas === document.querySelector("#scene")), true);
        assert.deepEqual(await page.evaluate(() => __noonExampleGallery.runPhaseMetrics.reconciliations), []);
        entry.checks.push("terminal-adoption-keeps-reader-and-pause", "no-completion-reconciliation");
        await seekSealed(page, 61);
        assert.equal(baseline.equals(await pixels(page, `${backend}-${transport}-terminal`)), false);
        await seekSealed(page, 5);
        assert.ok(baseline.equals(await pixels(page, `${backend}-${transport}-rewind`)));
        entry.checks.push("exact-endpoint", "exact-rewind-pixels");

        // A real Run replaces the historical stream, retaining the healthy VM.
        await page.locator("#replace-scene").click(); await openReady(page); await pause(page);
        assert.equal(await page.evaluate(() => originalGalleryCanvas === document.querySelector("#scene")), false);
        assert.equal(await page.evaluate(() => galleryWorkers.filter(name => name === "noon-python-authoring").length), 1);
        entry.checks.push("completed-rerun-uses-fresh-reader-warm-interpreter");

        // Select another source while preparation is demand-paused. This uses
        // normal cooperative cancellation and the gallery's generation gates.
        await selectGalleryExample(page, "prepared-failure");
        await page.waitForFunction(() => __noonExampleGallery.selectedExampleId === "prepared-failure");
        await openReady(page); await pause(page); await seekOpen(page, 29.5);
        await page.waitForFunction(() => !__noonExampleGallery.runInFlight &&
          document.querySelector("#patch-status")?.dataset.state === "error");
        await assertGallerySourceFailure(page);
        await openReady(page); await seekOpen(page, 5);
        assert.equal(await page.evaluate(() => galleryWorkers.filter(name => name === "noon-python-authoring").length), 1);
        assert.ok(baseline.equals(await pixels(page, `${backend}-${transport}-partial-history`)));
        entry.checks.push("gallery-selection-cancels-demand-wait", "source-error-retains-open-seekable-history");
        await selectGalleryExample(page, "prepared-lifecycle");
        await page.waitForFunction(() => __noonExampleGallery.selectedExampleId === "prepared-lifecycle");
        await openReady(page); await pause(page);
        assert.equal(await page.evaluate(() => galleryWorkers.filter(name => name === "noon-python-authoring").length), 1);
        assert.deepEqual(pageErrors, []);
        entry.checks.push("new-run-after-source-failure", "no-page-errors");
        await selectGalleryExample(page, "prepared-static");
        await page.waitForFunction(() => !__noonExampleGallery.runInFlight &&
          document.querySelector(".playback-controls")?.dataset.durationKnown === "true" &&
          document.querySelector(".playback-scrubber")?.max === "0");
        assert.equal(await page.locator("#patch-status").getAttribute("data-state"), "applied");
        assert.equal(await page.locator(".playback-toggle").isDisabled(), true);
        assert.equal(await page.locator(".playback-scrubber").isDisabled(), true);
        assert.equal(await page.locator(".playback-controls").getAttribute("data-playing"), "false");
        const staticPixels = await pixels(page, `${backend}-${transport}-static`);
        assert.ok(baseline.equals(staticPixels), "completed static reader must render the same circle");
        await page.locator(".playback-restart").click();
        await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
        assert.ok(staticPixels.equals(await pixels(page, `${backend}-${transport}-static-restart`)));
        assert.equal(staticInvocations, 1, "restart must not rerun static source");
        const staticGeneration = await page.evaluate(() => __noonExampleGallery.runPhaseMetrics.runGeneration);
        await page.locator("#replace-scene").click();
        await page.waitForFunction(previous => !__noonExampleGallery.runInFlight &&
          __noonExampleGallery.runPhaseMetrics.runGeneration > previous &&
          document.querySelector("#patch-status")?.dataset.state === "applied", staticGeneration);
        assert.equal(staticInvocations, 2, "explicit Run executes static source once");
        assert.equal(await page.evaluate(() => galleryWorkers.filter(name => name === "noon-python-authoring").length), 1);
        assert.deepEqual(pageErrors, []);
        entry.checks.push("completed-static-reader-exact-zero-pixels", "static-restart-never-reruns-source", "static-rerun-retains-warm-interpreter");
        await selectGalleryExample(page, "prepared-zero");
        await page.waitForFunction(() => !__noonExampleGallery.runInFlight &&
          document.querySelector(".playback-controls")?.dataset.durationKnown === "true" &&
          document.querySelector(".playback-scrubber")?.max === "0");
        assert.equal(await page.locator("#patch-status").getAttribute("data-state"), "applied");
        const zeroPixels = await pixels(page, `${backend}-${transport}-zero-only`);
        assert.ok(baseline.equals(zeroPixels), "zero-only reader must include final same-time source edits");
        await page.locator(".playback-restart").click();
        await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
        assert.ok(zeroPixels.equals(await pixels(page, `${backend}-${transport}-zero-restart`)));
        assert.equal(zeroInvocations, 1);
        const zeroGeneration = await page.evaluate(() => __noonExampleGallery.runPhaseMetrics.runGeneration);
        await page.locator("#replace-scene").click();
        await page.waitForFunction(previous => !__noonExampleGallery.runInFlight &&
          __noonExampleGallery.runPhaseMetrics.runGeneration > previous &&
          document.querySelector("#patch-status")?.dataset.state === "applied", zeroGeneration);
        assert.equal(zeroInvocations, 2);
        assert.deepEqual(pageErrors, []);
        entry.checks.push("initial-zero-wait-before-open-prefix", "zero-only-final-edits-exact-pixels", "zero-only-restart-and-rerun");
        entry.status = "passed";
      } catch (error) {
        entry.status = "failed"; entry.error = String(error.stack ?? error);
        entry.pageErrors = pageErrors;
        entry.diagnostics = await page.evaluate(() => ({
          status: document.querySelector("#status")?.dataset,
          message: document.querySelector("#patch-status")?.value,
          diagnosticTitle: document.querySelector("#authoring-problem > strong")?.textContent,
          diagnosticMessage: document.querySelector("#authoring-problem > p")?.textContent,
          run: globalThis.__noonExampleGallery?.runPhaseMetrics,
          controls: document.querySelector(".playback-controls")?.dataset,
        })).catch(() => null);
        await page.screenshot({ path: path.join(output, `${backend}-${transport}-failure.png`) }).catch(() => {});
        throw error;
      } finally { await page.close(); await server.close(); server = null; }
    }
    await browser.close(); browser = null;
  }
  report.status = "passed";
} catch (error) {
  report.status = "failed"; report.error = String(error.stack ?? error); process.exitCode = 1;
} finally {
  report.resourceCache = cache.stats();
  await writeFile(path.join(output, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
  await browser?.close(); await server?.close();
}
console.log(JSON.stringify(report));