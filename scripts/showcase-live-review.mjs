// Review ordinary gallery playback, not deterministic sampling or an FPS benchmark.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function assertLiveOutcome(entry, observed, expectedBackend) {
  assert.equal(observed.selectedExampleId, entry.id, "wrong live lesson");
  assert.equal(observed.patchState, "applied", "source did not finish successfully");
  assert.equal(observed.runInFlight, false, "source is still running");
  assert.equal(observed.backend, expectedBackend, "wrong live backend");
  assert.equal(observed.controls?.controllable, "true", "live lesson cannot replay");
  assert.equal(observed.controls.busy, "false", "playback command is unfinished");
  assert.equal(observed.controls.playing, "false", "capture is not paused");
  const duration = Number(observed.duration);
  const elapsed = Number(observed.controls.elapsedSeconds);
  assert.ok(Number.isFinite(duration) && duration > 0 && Math.abs(duration - entry.duration) < 1e-7,
    "live authored duration differs from the storyboard");
  assert.ok(Number.isFinite(elapsed) && Math.abs(elapsed - duration) < 1e-7,
    "live controls did not reach the authored endpoint");
  assert.ok(Number.isSafeInteger(observed.objectCount) && observed.objectCount > 0,
    "live lesson has no resolved composition");
  if (entry.performance) assert.ok(observed.objectCount >= 600, "dense scene lost its geometry workload");
}

async function main() {
  const { chromium } = await import("playwright");
  const { PNG } = (await import("pngjs")).default;
  const { serveRepository } = await import("./browser-test-server.mjs");
  const { browserArgs } = await import("./manim-raster-support.mjs");
  const { createPyodideResourceCache } = await import("./pyodide-resource-cache.mjs");
  const { normalizeShowcaseManifest } = await import("../web/showcase-gallery.js");
  const { assertCaptureTime } = await import("./showcase-capture-checks.mjs");
  const { seekPausedGallery } = await import("./showcase-playback.mjs");
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const manifest = JSON.parse(await readFile(path.join(root, "web/python/examples/noon_showcase_manifest.json"), "utf8"));
  normalizeShowcaseManifest(manifest);
  const backend = process.env.NOON_SHOWCASE_BACKEND ?? "webgl";
  assert.ok(["webgl", "webgpu"].includes(backend));
  const expectedBackend = backend === "webgl" ? "WebGL2" : "WebGPU";
  const output = path.join(root, "browser-smoke-artifacts/showcase", backend, "live");
  const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
  const json = (value) => JSON.stringify(value, (_, item) => typeof item === "bigint" ? String(item) : item, 2);
  const report = {
    purpose: "normal-playback video and replay review; recorded wall time is not a performance benchmark",
    viewport: { width: 1600, height: 1000 }, backendRequested: backend, results: [],
  };
  await mkdir(output, { recursive: true });
  const server = await serveRepository(root, 0);
  const base = `${server.baseUrl}/web`;
  let browser, context;
  try {
    browser = await chromium.launch({ headless: true, args: browserArgs(backend) });
    context = await browser.newContext({
      viewport: report.viewport, deviceScaleFactor: 1,
      recordVideo: { dir: path.join(output, "recordings"), size: report.viewport },
    });
    report.browserVersion = browser.version();
    const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.js"), "utf8"));
    await cache.install(context);
    const identity = await context.request.get(`${base}/runtime-build-identity.json`);
    assert.ok(identity.ok(), "served build identity is missing");
    report.servedBuildIdentity = await identity.json();
    for (const entry of manifest.entries) {
      const result = { id: entry.id, outcome: "fail", stage: "open", pageErrors: [] };
      report.results.push(result);
      const page = await context.newPage();
      page.setDefaultTimeout(120000);
      page.on("pageerror", (error) => result.pageErrors.push(String(error)));
      const video = page.video();
      try {
        await page.goto(`${base}/index.html?catalog=showcase&example=${entry.id}`);
        await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
        const source = await readFile(path.join(root, "web", entry.path), "utf8");
        const editorSource = await page.locator("#python-scene-source").inputValue();
        assert.equal(editorSource, source, "live review must run the exact checked-in source");
        result.sourceSha256 = hash(source);
        result.stage = "ordinary first pass";
        const started = performance.now();
        let timer;
        try {
          // Closing the page in finally retires its workers on timeout; no stuck run survives.
          await Promise.race([
            page.evaluate(() => window.__noonExampleGallery.run()),
            new Promise((_, reject) => {
              timer = setTimeout(() => reject(new Error("ordinary first pass exceeded its review deadline")), 360000);
            }),
          ]);
        } finally {
          clearTimeout(timer);
        }
        result.firstPassWallMsIncludingAuthoring = performance.now() - started;
        result.stage = "resolved endpoint";
        const requestedTime = await seekPausedGallery(page, entry.duration);
        const metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
        const observed = await page.evaluate(() => ({
          selectedExampleId: window.__noonExampleGallery.selectedExampleId,
          runInFlight: window.__noonExampleGallery.runInFlight,
          patchState: document.querySelector("#patch-status")?.dataset.state,
          backend: document.querySelector("#status")?.dataset.rendererBackend,
          controls: { ...document.querySelector(".playback-controls")?.dataset },
          duration: document.querySelector(".playback-scrubber")?.max,
        }));
        observed.objectCount = metrics.metrics.objectCount;
        assertLiveOutcome(entry, observed, expectedBackend);
        assertCaptureTime(entry, { requestedTime, publishedTime: metrics.metrics.time }, requestedTime);
        result.observed = observed;
        result.requestedTime = requestedTime;
        result.publishedTime = metrics.metrics.time;
        const canvas = page.locator("#scene");
        const first = await canvas.screenshot();
        const firstPixels = PNG.sync.read(first);
        assert.ok(firstPixels.width >= 320 && firstPixels.height >= 180, "live canvas is too small");
        await writeFile(path.join(output, `${entry.id}-endpoint.png`), first);
        result.endpointPixelSha256 = hash(firstPixels.data);
        result.stage = "restart and recover endpoint";
        await page.getByRole("button", { name: "Restart animation from the beginning", exact: true }).click();
        await seekPausedGallery(page, entry.duration);
        const replay = PNG.sync.read(await canvas.screenshot());
        assert.equal(replay.width, firstPixels.width);
        assert.equal(replay.height, firstPixels.height);
        assert.ok(replay.data.equals(firstPixels.data), "replay did not reproduce the resolved pixels");
        assert.deepEqual(result.pageErrors, []);
        result.restartRestoresEndpoint = true;
        result.outcome = "pass";
        console.log(`PASS live ${entry.id}: normal source run and replay endpoint`);
      } catch (error) {
        result.error = String(error.stack ?? error);
        result.failureState = await page.evaluate(() => ({
          patch: document.querySelector("#patch-status")?.value,
          status: document.querySelector("#status-text")?.textContent,
          controls: { ...document.querySelector(".playback-controls")?.dataset },
        })).catch(() => null);
        await page.screenshot({ path: path.join(output, `${entry.id}-failure.png`) }).catch(() => {});
        console.error(`FAIL live ${entry.id}: ${result.error}`);
      } finally {
        await page.close();
        try {
          assert.ok(video, "review video was not created");
          result.video = `${entry.id}.webm`;
          await video.saveAs(path.join(output, result.video));
          await video.delete();
        } catch (error) {
          result.outcome = "fail";
          result.videoError = String(error);
        }
        await writeFile(path.join(output, "report.json"), json(report));
      }
    }
    report.runtimeResources = cache.stats();
    assert.deepEqual(report.results.filter((result) => result.outcome !== "pass").map((result) => result.id), [],
      "normal-playback review failures");
  } finally {
    await writeFile(path.join(output, "report.json"), json(report));
    await context?.close();
    await browser?.close();
    await server.close();
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
