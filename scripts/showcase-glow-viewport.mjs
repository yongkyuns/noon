// Exercise the production gallery shell before pixel-oracle tests normalize it.
// No border removal, integer canvas forcing, bitmap writes or fake renderer.
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

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
    const result = { fixture, outcome: "fail", pageErrors: [] };
    report.results.push(result);
    page.on("pageerror", error => result.pageErrors.push(String(error)));
    try {
      await page.goto(`${base}/index.html?catalog=showcase&example=showcase-gpu-glow`);
      if (fixture.resize) {
        await page.waitForFunction(() => document.querySelector("#patch-status")?.dataset.state === "error" ||
          Number(document.querySelector(".playback-controls")?.dataset.elapsedSeconds) >= 2,
        null, { timeout: 120000 });
        result.resizeAt = await page.evaluate(() => ({
          elapsed: document.querySelector(".playback-controls")?.dataset.elapsedSeconds,
          sourceRunning: window.__noonExampleGallery?.runInFlight,
        }));
        assert.equal(result.resizeAt.sourceRunning, true, "resize must occur during source execution");
        await page.setViewportSize(fixture.resize);
      }
      await page.waitForFunction(() => {
        const state = document.querySelector("#patch-status")?.dataset.state;
        return state === "error" || (state === "applied" &&
          window.__noonExampleGallery?.selectedExampleId === "showcase-gpu-glow" &&
          !window.__noonExampleGallery.runInFlight);
      }, null, { timeout: 120000 });
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
