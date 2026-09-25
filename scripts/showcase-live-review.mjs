// Review ordinary gallery playback and compare replay against independent
// forward captures. Neither sampled frames nor video timings are an FPS benchmark.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { replayOracle, assertOracleImage, assertReplaySample } from "./showcase-replay-checks.mjs";
import { waitForPublishedGalleryFrame } from "./showcase-playback.mjs";
import { layoutReplayViewport, replayViewport, qualifyReplayViewport } from "./showcase-viewport.mjs";

// First execution and replay are distinct engine capabilities. Keep both results,
// but do not count a successful first pass as a replacement for failed replay.
export function assertFirstPass(entry, observed, expectedBackend) {
  assert.equal(observed.selectedExampleId, entry.id, "wrong first-pass lesson");
  assert.equal(observed.patchState, "applied", "first-pass source did not finish successfully");
  assert.equal(observed.runInFlight, false, "first-pass source is still running");
  assert.equal(observed.backend, expectedBackend, "wrong first-pass backend");
  assert.ok(Number.isSafeInteger(observed.objectCount) && observed.objectCount > 0,
    "first-pass lesson has no resolved composition");
  if (entry.performance) assert.ok(observed.objectCount >= 600, "dense scene lost its geometry workload");
  const elapsed = Number(observed.controls?.elapsedSeconds);
  const roundoff = 8 * Number.EPSILON * Math.max(1, entry.duration);
  assert.ok(Number.isFinite(elapsed) && Math.abs(elapsed - entry.duration) <= roundoff,
    "first-pass source did not reach its authored endpoint");
}

export function assertReplayAvailable(observed) {
  assert.equal(observed.controls?.controllable, "true",
    observed.replayReason || "completed source did not admit retained replay");
}

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

// A live seek targets the actual authored endpoint, not its decimal storyboard.
// Permit only floating-point roundoff between those two representations. Ordinary
// deterministic samples retain their separate, strict assertCaptureTime policy.
export function assertLiveEndpoint(entry, authoredDuration, requestedTime, publishedTime) {
  for (const value of [entry.duration, authoredDuration, requestedTime, publishedTime]) {
    assert.ok(Number.isFinite(value) && value > 0, "invalid live endpoint time");
  }
  const roundoff = 8 * Number.EPSILON * Math.max(1, entry.duration, authoredDuration);
  assert.ok(Math.abs(authoredDuration - entry.duration) <= roundoff,
    "actual duration differs from the storyboard beyond floating-point roundoff");
  assert.equal(requestedTime, authoredDuration, "live seek did not target the actual endpoint");
  assert.ok(Math.abs(publishedTime - authoredDuration) <= roundoff,
    "live endpoint was not presented at its authored time");
}

// Both replay paths must reproduce ordinary execution, not just agree with each other.
export function assertLivePixels(firstPass, replay) {
  assert.equal(replay.width, firstPass.width, "replay width differs from first pass");
  assert.equal(replay.height, firstPass.height, "replay height differs from first pass");
  assert.ok(replay.data.equals(firstPass.data), "replay differs from unseeked first-pass pixels");
}

async function main() {
  const { chromium } = await import("playwright");
  const { PNG } = (await import("pngjs")).default;
  const { serveRepository } = await import("./browser-test-server.mjs");
  const { browserArgs } = await import("./manim-raster-support.mjs");
  const { createPyodideResourceCache } = await import("./pyodide-resource-cache.mjs");
  const { normalizeShowcaseManifest } = await import("../web/showcase-gallery.js");
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
    // Keep collecting ordinary playback even if the independent layout fixture
    // fails. Its result remains fatal to the overall qualification.
    try {
      const shell = await readFile(path.join(root, "web/index.html"), "utf8");
      report.viewportQualification = { outcome: "pass", ...await qualifyReplayViewport(browser, PNG.sync.read, shell) };
    } catch (error) {
      report.viewportQualification = { outcome: "fail", error: String(error.stack ?? error) };
    }
    const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.js"), "utf8"));
    await cache.install(context);
    const identity = await context.request.get(`${base}/runtime-build-identity.json`);
    assert.ok(identity.ok(), "served build identity is missing");
    report.servedBuildIdentity = await identity.json();
    // The capture job runs immediately before this one against the same build.
    // Missing/invalid evidence fails each lesson but must not prevent collecting
    // ordinary-playback videos and existing endpoint diagnostics for the rest.
    let captureReport;
    try {
      const bytes = await readFile(path.join(output, "..", "report.json"));
      captureReport = JSON.parse(bytes);
      report.firstPassCaptureReportSha256 = hash(bytes);
    } catch (error) {
      report.firstPassCaptureReportError = String(error);
    }
    for (const entry of manifest.entries) {
      const result = { id: entry.id, outcome: "fail", firstPassOutcome: "not-run", replayOutcome: "not-run", intermediateReplayOutcome: "not-run", stage: "open", pageErrors: [] };
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
        result.firstPassOutcome = "fail";
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
        // Preserve the unseeked result even when the runtime refuses replay.
        // An unavailable replay remains a gate failure, not a successful fallback.
        result.firstPass = await page.evaluate(() => ({
          selectedExampleId: window.__noonExampleGallery.selectedExampleId,
          runInFlight: window.__noonExampleGallery.runInFlight,
          patchState: document.querySelector("#patch-status")?.dataset.state,
          patch: document.querySelector("#patch-status")?.value,
          backend: document.querySelector("#status")?.dataset.rendererBackend,
          controls: { ...document.querySelector(".playback-controls")?.dataset },
          replayReason: document.querySelector(".playback-controls")?.title,
        }));
        const canvas = page.locator("#scene");
        const firstPass = PNG.sync.read(await canvas.screenshot({
          path: path.join(output, `${entry.id}-first-pass.png`),
        }));
        result.firstPassPixelSha256 = hash(firstPass.data);
        const firstMetrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
        result.firstPass.objectCount = firstMetrics.metrics.objectCount;
        assertFirstPass(entry, result.firstPass, expectedBackend);
        assert.deepEqual(result.pageErrors, [], "ordinary source execution raised browser errors");
        result.firstPassOutcome = "pass";
        result.stage = "replay capability admission";
        result.replayOutcome = "fail";
        assertReplayAvailable(result.firstPass);
        result.stage = "replay seek to resolved endpoint";
        const requestedTime = await seekPausedGallery(page, entry.duration);
        const metrics = await waitForPublishedGalleryFrame(page, requestedTime, entry.duration);
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
        assertLiveEndpoint(entry, Number(observed.duration), requestedTime, metrics.metrics.time);
        result.observed = observed;
        result.requestedTime = requestedTime;
        result.publishedTime = metrics.metrics.time;
        const endpoint = PNG.sync.read(await canvas.screenshot({
          path: path.join(output, `${entry.id}-endpoint.png`),
        }));
        result.endpointPixelSha256 = hash(endpoint.data);
        assert.ok(endpoint.width >= 320 && endpoint.height >= 180, "live canvas is too small");
        assertLivePixels(firstPass, endpoint);
        result.endpointMatchesFirstPass = true;
        result.stage = "restart and recover endpoint";
        await page.getByRole("button", { name: "Restart animation from the beginning", exact: true }).click();
        const replayTime = await seekPausedGallery(page, entry.duration);
        const replayMetrics = await waitForPublishedGalleryFrame(page, replayTime, entry.duration);
        assertLiveEndpoint(entry, Number(observed.duration), replayTime, replayMetrics.metrics.time);
        const replay = PNG.sync.read(await canvas.screenshot({
          path: path.join(output, `${entry.id}-restart.png`),
        }));
        result.restartPixelSha256 = hash(replay.data);
        assertLivePixels(firstPass, replay);
        result.restartMatchesFirstPass = true;
        assert.deepEqual(result.pageErrors, []);
        result.restartRestoresEndpoint = true;
        result.stage = "independent intermediate replay qualification";
        result.intermediateReplayOutcome = "fail";
        const oracle = replayOracle(entry, captureReport, {
          sourceSha256: result.sourceSha256, buildIdentity: report.servedBuildIdentity,
          browserVersion: report.browserVersion, backendRequested: backend,
        });
        // Keep the ordinary-playback recording and endpoint checks at the real
        // gallery size. Only this additional comparison resizes the actual
        // canvas to the forward oracle's viewport; never rescale/crop PNGs.
        await layoutReplayViewport(canvas, captureReport.viewport);
        await seekPausedGallery(page, entry.duration, 0);
        await waitForPublishedGalleryFrame(page, 0, entry.duration);
        result.intermediateViewport = await replayViewport(canvas, captureReport.viewport);
        result.intermediateSamples = [];
        for (const [direction, checkpoints] of [["backward", [...oracle].reverse()], ["forward", oracle]]) {
          for (const [index, checkpoint] of checkpoints.entries()) {
            const comparison = { direction, firstPassFilename: checkpoint.filename,
              firstPassRequestedTime: checkpoint.requestedTime, firstPassPublishedTime: checkpoint.publishedTime,
              firstPassPixelSha256: checkpoint.pixelSha256, requestedTime: checkpoint.replayTime, outcome: "fail" };
            result.intermediateSamples.push(comparison);
            try {
              const firstBytes = await readFile(path.join(output, "..", checkpoint.filename));
              const firstImage = PNG.sync.read(firstBytes);
              assertOracleImage(checkpoint, firstBytes, firstImage);
              const time = await seekPausedGallery(page, entry.duration, checkpoint.completionProbe ? null : checkpoint.replayTime);
              const replayMetrics = await waitForPublishedGalleryFrame(page, time, entry.duration);
              comparison.publishedTime = replayMetrics.metrics.time;
              comparison.backend = replayMetrics.metrics.backend;
              comparison.filename = `${entry.id}-${direction}-${index}.png`;
              const image = PNG.sync.read(await canvas.screenshot({ path: path.join(output, comparison.filename) }));
              comparison.pixelSha256 = hash(image.data);
              comparison.width = image.width;
              comparison.height = image.height;
              assertReplaySample(entry, checkpoint, time, replayMetrics.metrics);
              assertLivePixels(firstImage, image);
              comparison.outcome = "pass";
            } catch (error) {
              comparison.error = String(error.stack ?? error);
            }
          }
        }
        assert.deepEqual(result.pageErrors, []);
        assert.deepEqual(result.intermediateSamples.filter(sample => sample.outcome !== "pass")
          .map(sample => `${sample.direction}@${sample.requestedTime}`), [], "intermediate replay differs from first execution");
        result.intermediateReplayOutcome = "pass";
        result.replayOutcome = "pass";
        result.outcome = "pass";
        console.log(`PASS live ${entry.id}: normal source run, replay endpoint, and ${result.intermediateSamples.length} intermediate comparisons`);
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
    assert.equal(report.viewportQualification.outcome, "pass", "browser capture layout fixture failed");
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
