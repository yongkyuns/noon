// Capture the exact showcase sources through existing Noon hosts. No Manim renders,
// synthetic posters, private scene mutation, or performance conclusions from sampled time.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import pngjs from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { normalizeShowcaseManifest } from "../web/showcase-gallery.js";
import { assertCaptureTime, assertCompletedCapture, captureSchedule } from "./showcase-capture-checks.mjs";

const { PNG } = pngjs;
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(await readFile(path.join(root, "web/python/examples/noon_showcase_manifest.json"), "utf8"));
normalizeShowcaseManifest(manifest);
const schedules = new Map(manifest.entries.map(entry => [entry.id, captureSchedule(entry)]));
const backend = process.env.NOON_SHOWCASE_BACKEND ?? "webgl";
assert.ok(["webgl", "webgpu"].includes(backend));
const expectedBackend = backend === "webgl" ? "WebGL2" : "WebGPU";
const output = path.join(root, "browser-smoke-artifacts/showcase", backend);
const report = {
  purpose: "render-and-presentation-qualification, not a real-time performance benchmark",
  checkoutRevision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  backendRequested: backend, samplingHz: 30, viewport: { width: 960, height: 540 }, results: [],
};
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const json = (value) => JSON.stringify(value, (_, item) => typeof item === "bigint" ? String(item) : item, 2);
const escape = (text) => String(text).replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character]);

function validateImage(bytes, label) {
  const image = PNG.sync.read(bytes);
  assert.ok(image.width >= 320 && image.height >= 180, `${label}: unexpectedly small capture`);
  const background = image.data.subarray(0, 3);
  let foreground = 0;
  for (let index = 0; index < image.data.length; index += 4) {
    if ([0, 1, 2].some((channel) => Math.abs(image.data[index + channel] - background[channel]) > 20)) foreground++;
  }
  assert.ok(foreground > image.width * image.height * 0.001, `${label}: effectively empty capture`);
  return { width: image.width, height: image.height, foregroundPixels: foreground,
    pngSha256: hash(bytes), pixelSha256: hash(image.data) };
}

function samePixels(left, right) {
  const a = PNG.sync.read(left), b = PNG.sync.read(right);
  return a.width === b.width && a.height === b.height && a.data.equals(b.data);
}

await mkdir(output, { recursive: true });
const server = await serveRepository(root, 0);
const base = `${server.baseUrl}/web`;
let browser;
try {
  browser = await chromium.launch({ headless: true, args: browserArgs(backend) });
  report.browserVersion = browser.version();
  const context = await browser.newContext({ viewport: report.viewport, deviceScaleFactor: 1 });
  const worker = await readFile(path.join(root, "web/python-worker.js"), "utf8");
  const cache = createPyodideResourceCache(worker);
  await cache.install(context);
  const identity = await context.request.get(`${base}/runtime-build-identity.json`);
  assert.ok(identity.ok(), "served runtime build identity missing");
  report.servedBuildIdentity = await identity.json();
  report.workerFileSha256 = hash(worker);

  for (const entry of manifest.entries) {
    const result = { id: entry.id, title: entry.title, samples: [], pageErrors: [], outcome: "fail" };
    report.results.push(result);
    const page = await context.newPage();
    page.setDefaultTimeout(120000);
    page.on("pageerror", (error) => result.pageErrors.push(String(error)));
    try {
      const source = await readFile(path.join(root, "web", entry.path), "utf8");
      result.sourceSha256 = hash(source);
      await page.goto(`${base}/manim-raster-host.html`);
      await page.waitForFunction(() => window.noonHostRaster !== undefined);
      const loaded = await page.evaluate(({ source, duration }) => window.noonHostRaster.load(source, duration), { source, duration: entry.duration });
      assert.equal(loaded.rendererBackend, expectedBackend, `${entry.id}: requested backend must actually execute`);
      const { frameTimes, wanted, completionTime, posterTime } = schedules.get(entry.id);
      let poster;
      for (const time of wanted) {
        const frameIndex = frameTimes.indexOf(time);
        const completionProbe = time === completionTime;
        const sample = await page.evaluate(({ frameIndex, frameTimes, completionProbe }) =>
          window.noonHostRaster.renderThrough(frameIndex, frameTimes, { stopAtSourceCompletion: completionProbe }),
        { frameIndex, frameTimes, completionProbe });
        assert.equal(sample.error, null);
        assert.equal(sample.presented, true);
        assert.equal(sample.rendererBackend, expectedBackend);
        if (completionProbe) assertCompletedCapture(entry, sample, time);
        else assertCaptureTime(entry, sample, time);
        // A completion probe stops at the actual endpoint. Ordinary quiet holds
        // may reuse a frame only within their explicitly declared still interval.
        const bytes = await page.locator("#scene").screenshot();
        const filename = `${entry.id}-${String(time).replace(".", "_")}.png`;
        const image = validateImage(bytes, `${entry.id}@${time}`);
        await writeFile(path.join(output, filename), bytes);
        result.samples.push({ ...sample, completionProbe, ...image, filename });
        if (time === posterTime) poster = bytes;
        if (entry.performance && time >= 3.1) assert.ok(sample.objectCount >= 600, `${entry.id}: dense phases must retain the geometry workload`);
      }
      assert.ok(new Set(result.samples.map((sample) => sample.pixelSha256)).size >= 3, `${entry.id}: temporal samples did not change`);
      await page.evaluate(() => window.noonHostRaster.close());
      assert.deepEqual(result.pageErrors, []);
      if (entry.interaction) {
        poster = await captureSelection(context, entry, result);
      }
      assert.ok(poster, `${entry.id}: poster not captured`);
      result.poster = `${entry.id}.png`;
      result.posterImage = validateImage(poster, `${entry.id}: poster`);
      await writeFile(path.join(output, result.poster), poster);
      result.outcome = "pass";
      console.log(`PASS ${entry.id}: ${result.samples.length} actual ${expectedBackend} samples`);
    } catch (error) {
      result.error = String(error.stack ?? error);
      console.error(`FAIL ${entry.id}: ${result.error}`);
      await page.screenshot({ path: path.join(output, `${entry.id}-failure.png`) }).catch(() => {});
    } finally {
      await page.close();
      await writeFile(path.join(output, "report.json"), json(report));
    }
  }

  const cards = report.results.map((result) => `<article><h2>${escape(result.title)}</h2><p>${escape(result.outcome)}</p>${result.poster ? `<img src="${result.poster}" alt="${escape(result.title)}">` : "<p>Capture failed — not publishable</p>"}<details><summary>Storyboard frames</summary>${result.samples.map((sample) => `<figure><img src="${sample.filename}" alt="Actual scene frame"><figcaption>${sample.completionProbe ? "Completion probe" : "Requested"} ${sample.requestedTime}s · published ${sample.publishedTime}s</figcaption></figure>`).join("")}</details></article>`).join("");
  await writeFile(path.join(output, "index.html"), `<!doctype html><html lang="en"><meta charset="utf-8"><title>Noon showcase review</title><style>body{margin:32px;font:16px system-ui;background:#10141d;color:#eef2fa}main{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:20px}article{border:1px solid #3b4454;padding:12px}h2{font-size:18px}img{width:100%;height:auto}figure{margin:12px 0}figcaption{font-size:12px}</style><h1>Noon showcase — actual ${escape(expectedBackend)} captures</h1><p>Preview review, not editorial approval or performance benchmarking.</p><main>${cards}</main></html>`);
  const sheet = await context.newPage();
  await sheet.setViewportSize({ width: 1440, height: 1200 });
  await sheet.goto(`${server.baseUrl}/browser-smoke-artifacts/showcase/${backend}/index.html`);
  await sheet.evaluate(async () => { await Promise.all([...document.images].map((image) => image.decode())); });
  await sheet.screenshot({ path: path.join(output, "contact-sheet.png"), fullPage: true });
  await sheet.close();
  const failed = report.results.filter((result) => result.outcome !== "pass");
  assert.deepEqual(failed.map((result) => result.id), [], "showcase capture failures");
  // Stage real posters only after every scene passes. They remain review artifacts;
  // this script does not commit, approve, publish, or modify the catalog's status.
  await mkdir(path.join(root, "web/thumbnails/showcase"), { recursive: true });
  for (const result of report.results) {
    await writeFile(path.join(root, "web/thumbnails/showcase", result.poster), await readFile(path.join(output, result.poster)));
  }
  const decodePage = await context.newPage();
  await decodePage.goto(`${base}/manim-raster-host.html`);
  await decodePage.evaluate(async (paths) => {
    await Promise.all(paths.map(async (path) => {
      const image = new Image(); image.src = path; await image.decode();
      if (image.naturalWidth < 320 || image.naturalHeight < 180) throw new Error(`Invalid poster: ${path}`);
    }));
  }, manifest.entries.map((entry) => `./${entry.thumbnail}`));
  await decodePage.close();
  report.stagedPosterDecode = "pass";
  report.runtimeResources = cache.stats();
  await context.close();
} finally {
  await writeFile(path.join(output, "report.json"), json(report));
  await browser?.close();
  await server.close();
}

async function captureSelection(context, entry, result) {
  const page = await context.newPage();
  page.setDefaultTimeout(120000);
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  try {
    await page.setViewportSize({ width: 1800, height: 1100 });
    await page.goto(`${base}/index.html?catalog=showcase&example=${entry.id}`);
    await page.addStyleTag({ content: ".workspace{grid-template-columns:300px 1fr}.canvas-frame{width:960px;max-width:none}" });
    await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
    await page.evaluate(() => window.__noonExampleGallery.run());
    await page.waitForFunction(() => document.querySelector("#patch-status")?.dataset.state === "applied" && !window.__noonExampleGallery.runInFlight);
    const scrubber = page.locator(".playback-scrubber");
    await scrubber.evaluate((input) => { input.value = input.max; input.dispatchEvent(new Event("input", { bubbles: true })); });
    // The scrubber deliberately stays enabled while seeking to coalesce input.
    // Wait on the existing command-completion state, not the input's disabled flag.
    await page.waitForFunction((duration) => {
      const controls = document.querySelector(".playback-controls");
      return controls?.dataset.busy === "false" &&
        Math.abs(Number(controls.dataset.elapsedSeconds) - duration) < 1e-6;
    }, entry.duration);
    const pause = page.getByRole("button", { name: "Pause animation", exact: true });
    if (await pause.count()) {
      await pause.click();
      await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
    }
    assert.equal(await page.locator("#patch-status").getAttribute("data-state"), "applied");
    const metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
    const actualBackend = await page.locator("#status").getAttribute("data-renderer-backend");
    assert.equal(actualBackend, expectedBackend, "pointer capture must use the requested backend too");
    assertCaptureTime(entry, { requestedTime: entry.duration, publishedTime: metrics.metrics.time }, entry.duration);
    const canvas = page.locator("#scene");
    const before = await canvas.screenshot();
    const bounds = await canvas.boundingBox();
    assert.ok(bounds);
    const click = (x, y) => page.mouse.click(bounds.x + bounds.width * x, bounds.y + bounds.height * y);
    await click(0.36, 0.5);
    let selected;
    for (let attempt = 0; attempt < 40; attempt++) {
      const bytes = await canvas.screenshot();
      if (!samePixels(before, bytes)) { selected = bytes; break; }
      await page.waitForTimeout(50);
    }
    assert.ok(selected, "actual pointer click did not change the displayed image");
    await click(0.05, 0.5);
    let cleared = false;
    for (let attempt = 0; attempt < 40; attempt++) {
      if (samePixels(before, await canvas.screenshot())) { cleared = true; break; }
      await page.waitForTimeout(50);
    }
    assert.ok(cleared, "background click did not restore the base pixels exactly");
    assert.deepEqual(errors, []);
    result.interaction = {
      recipe: "completed introduction -> click normalized (0.36, 0.5) -> clear (0.05, 0.5)",
      requestedTime: entry.duration, publishedTime: metrics.metrics.time,
      rendererBackend: actualBackend, exactClear: true, baseMetrics: metrics,
    };
    return selected;
  } finally {
    await page.close();
  }
}
