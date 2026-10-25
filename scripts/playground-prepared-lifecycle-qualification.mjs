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
const entries = ["prepared-lifecycle", "prepared-failure"].map((id, order) => ({
  id, order, title: id, status: "ready", parity_status: "candidate",
  reuse: "manim-compatible-parity-v0.21", category: "qualification",
  path: `python/examples/${id}.py`, thumbnail: "prepared-fixture.svg",
  features: ["prepared-lifecycle"], summary: "Bounded lifecycle qualification fixture",
}));
const hash = value => createHash("sha256").update(value).digest("hex");
const report = { status: "running", scope: "actual gallery lifecycle; bounded fixture, not gallery compatibility or performance",
  sourceSha256: hash(source), captureBackdrop: "opaque canvas-wrap; unchanged full-canvas RGBA equality", cases: [] };
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
      body: entry.id === "prepared-failure" ? failureSource : source }));
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
      try {
        await installFixture(page);
        await page.goto(`${server.baseUrl}/web/index.html?example=prepared-lifecycle`);
        await page.waitForFunction(() => globalThis.__noonExampleGallery !== undefined);
        // Rounded canvas corners and fractional edge coverage expose the page
        // backdrop. Its decorative gradient can dither between screenshots.
        // Make only that backdrop opaque; retain the canvas geometry, border,
        // renderer, full screenshot and exact pixel comparisons unchanged.
        await page.addStyleTag({ content: ".canvas-wrap { background: #090c13 !important; }" });
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
        assert.match(await page.locator("#patch-status").evaluate(element => element.value ?? element.textContent), /intentional gallery source failure/);
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
        entry.status = "passed";
      } catch (error) {
        entry.status = "failed"; entry.error = String(error.stack ?? error);
        entry.pageErrors = pageErrors;
        entry.diagnostics = await page.evaluate(() => ({
          status: document.querySelector("#status")?.dataset,
          message: document.querySelector("#patch-status")?.value,
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