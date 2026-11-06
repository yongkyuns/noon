// Exercise the production gallery shell before pixel-oracle tests normalize it.
// No border removal, integer canvas forcing, bitmap writes or fake renderer.
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";
import { seekPausedGallery } from "./showcase-playback.mjs";

export async function qualifyGlowViewport(browser, base, output, backend, cache) {
  const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
  const fixtures = [
    { name: "decorated-desktop", width: 1600, height: 1000, dpr: 1 },
    { name: "fractional-layout-and-density", width: 1100, height: 800, dpr: 1.25, noJspi: true },
    { name: "resize-with-active-glow", width: 1600, height: 1000, dpr: 2,
      resize: { width: 1103, height: 803 } },
  ];
  const report = { scope: "real gallery DOM and source execution, not physical GPU performance", results: [] };
  await mkdir(output, { recursive: true });
  for (const fixture of fixtures) {
    const context = await browser.newContext({
      viewport: { width: fixture.width, height: fixture.height }, deviceScaleFactor: fixture.dpr,
    });
    if (cache) await cache.install(context);
    if (fixture.noJspi) await disableAuthoringJspi(context);
    const page = await context.newPage();
    const result = { fixture, outcome: "fail", pageErrors: [], consoleErrors: [] };
    report.results.push(result);
    page.on("pageerror", error => result.pageErrors.push(String(error)));
    page.on("console", message => {
      if (message.type() === "error") result.consoleErrors.push(message.text());
    });
    try {
      await page.goto(`${base}/index.html?catalog=showcase&example=showcase-gpu-glow`);
      if (fixture.resize) {
        await page.waitForFunction(() => {
          if (document.querySelector("#patch-status")?.dataset.state === "error") return true;
          const controls = document.querySelector(".playback-controls");
          return Number(controls?.dataset.elapsedSeconds) >= 2 &&
            controls?.dataset.playing === "true" && controls.dataset.durationKnown === "true" &&
            !window.__noonExampleGallery?.runInFlight;
        }, null, { timeout: 120000 });
        result.resizeAt = await page.evaluate(() => ({
          elapsed: document.querySelector(".playback-controls")?.dataset.elapsedSeconds,
          sourceRunning: window.__noonExampleGallery?.runInFlight,
          playing: document.querySelector(".playback-controls")?.dataset.playing,
          preparing: document.querySelector(".playback-controls")?.dataset.preparing,
          buffering: document.querySelector(".playback-controls")?.dataset.buffering,
          durationKnown: document.querySelector(".playback-controls")?.dataset.durationKnown,
          timelineEnd: document.querySelector(".playback-scrubber")?.max,
        }));
        assert.equal(result.resizeAt.sourceRunning, false, "finite source must finish before the playback resize");
        assert.equal(result.resizeAt.playing, "true", "resize must occur during active playback");
        assert.equal(result.resizeAt.preparing, "false");
        assert.equal(result.resizeAt.buffering, "false");
        assert.equal(result.resizeAt.durationKnown, "true", "full source timeline must be available at resize");
        assert.ok(Math.abs(Number(result.resizeAt.timelineEnd) - 12) < 1e-7,
          "resize must retain the complete 12-second source timeline");
        await page.setViewportSize(fixture.resize);
      }
      await page.waitForFunction(() => {
        const state = document.querySelector("#patch-status")?.dataset.state;
        const controls = document.querySelector(".playback-controls");
        return state === "error" || (state === "applied" &&
          window.__noonExampleGallery?.selectedExampleId === "showcase-gpu-glow" &&
          !window.__noonExampleGallery.runInFlight &&
          controls?.dataset.busy === "false" && controls.dataset.durationKnown === "true" &&
          controls.dataset.buffering === "false" &&
          window.__noonExampleGallery.executionDiagnostics?.events.some(event =>
            event.stage === "disposition" && event.disposition === "looped" && event.loopCount >= 1));
      }, null, { timeout: 120000 });
      // Layout qualification selects its endpoint through the normal controls;
      // independent unseeked first-pass pixels are checked by the live review.
      if (await page.locator("#patch-status").getAttribute("data-state") !== "error") {
        await seekPausedGallery(page, 12);
      }
      result.state = await page.evaluate(() => {
        const canvas = document.querySelector("#scene");
        const rect = canvas.getBoundingClientRect();
        return {
          patch: document.querySelector("#patch-status")?.value,
          patchState: document.querySelector("#patch-status")?.dataset.state,
          status: document.querySelector("#status-text")?.textContent,
          backend: document.querySelector("#status")?.dataset.rendererBackend,
          elapsed: Number(document.querySelector(".playback-controls")?.dataset.elapsedSeconds),
          rect: { width: rect.width, height: rect.height },
          client: [canvas.clientWidth, canvas.clientHeight],
          bitmap: [canvas.width, canvas.height],
          border: getComputedStyle(canvas).borderLeftWidth,
          dpr: devicePixelRatio,
        };
      });
      assert.equal(result.state.patchState, "applied", JSON.stringify(result.state));
      assert.equal(result.state.backend, expectedBackend);
      assert.ok(Math.abs(result.state.elapsed - 12) < 1e-7, "glow source must finish its actual 12-second timeline");
      // Deliberately retain the shipped border and layout mismatch that the
      // normalized screenshot oracles cannot exercise.
      assert.ok(parseFloat(result.state.border) > 0, "fixture must keep production canvas decoration");
      const metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
      assert.equal(metrics.metrics.objectCount, 11);
      assert.ok(metrics.metrics.presentedFrames > 0);
      assert.deepEqual(result.pageErrors, []);
      assert.deepEqual(result.consoleErrors, []);
      result.outcome = "pass";
      console.log(`PASS glow viewport ${backend} ${fixture.name}: ${JSON.stringify(result.state)}`);
    } catch (error) {
      result.error = String(error.stack ?? error);
      console.error(`FAIL glow viewport ${backend} ${fixture.name}: ${result.error}`);
    } finally {
      await page.screenshot({ path: path.join(output, `${fixture.name}.png`) }).catch(() => {});
      await context.close();
      await writeFile(path.join(output, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
    }
  }
  assert.deepEqual(report.results.filter(result => result.outcome !== "pass").map(result => result.fixture.name), [],
    "production-layout glow viewport regression");
  return report;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { chromium } = await import("playwright");
  const { serveRepository } = await import("./browser-test-server.mjs");
  const { browserArgs } = await import("./manim-raster-support.mjs");
  const { createPyodideResourceCache } = await import("./pyodide-resource-cache.mjs");
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const backend = process.env.NOON_SHOWCASE_BACKEND ?? "webgpu";
  assert.ok(["webgpu", "webgl"].includes(backend));
  const server = await serveRepository(root, 0);
  const browser = await chromium.launch({ headless: true, args: browserArgs(backend) });
  try {
    const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.js"), "utf8"));
    await qualifyGlowViewport(browser, `${server.baseUrl}/web`,
      path.join(root, "browser-smoke-artifacts/showcase", backend, "production-viewport"), backend, cache);
  } finally {
    await browser.close();
    await server.close();
  }
}
