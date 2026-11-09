// Actual engine frames, sampled in scene time. These previews are not an FPS test.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import pngjs from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { assertCaptureTime } from "./showcase-capture-checks.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const readJson = async (name) => JSON.parse(await readFile(path.join(root, name), "utf8"));
const selections = await readJson("web/site/examples.json");
const gallery = await readJson("web/python/examples/noon_showcase_manifest.json");
const output = path.join(root, "web/site/media");
const scratch = await mkdtemp(path.join(os.tmpdir(), "noon-project-media-"));
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const fps = 30;
const report = {
  purpose: "sampled real Noon output; not a real-time performance benchmark",
  checkoutRevision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  runtime: await readJson("web/runtime-build-identity.json"),
  rendererBackend: "WebGL2", samplingHz: 30, encodedFps: { gif: 10, mp4: fps },
  viewport: { width: 960, height: 540 }, results: [],
};
assert.equal(report.runtime.sourceRevision, report.checkoutRevision, "capture runtime must match this checkout");
const server = await serveRepository(root, 0);
let browser;
try {
  // Fail before creating publication assets if the encoder is unavailable.
  report.encoder = execFileSync("ffmpeg", ["-version"], { encoding: "utf8" }).split("\n")[0];
  browser = await chromium.launch({ headless: true, args: browserArgs("webgl") });
  report.browserVersion = browser.version();
  const context = await browser.newContext({ viewport: report.viewport, deviceScaleFactor: 1 });
  const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.js"), "utf8"));
  await cache.install(context);
  for (const selection of selections) {
    const { id, start, end } = selection;
    const entry = gallery.entries.find(candidate => candidate.id === id);
    assert.ok(entry && start >= 0 && end > start && end <= entry.duration, `${id}: invalid clip`);
    assert.ok([start, end].every(time => Number.isInteger(time * 30)), `${id}: clip must use the 30 Hz grid`);
    const source = await readFile(path.join(root, "web", entry.path), "utf8");
    const frames = path.join(scratch, id);
    await mkdir(frames);
    const page = await context.newPage();
    page.setDefaultTimeout(120_000);
    const errors = [];
    page.on("pageerror", error => errors.push(String(error)));
    const samples = [];
    try {
      await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
      await page.waitForFunction(() => window.noonHostRaster !== undefined);
      const loaded = await page.evaluate(({ source, duration }) => window.noonHostRaster.load(source, duration),
        { source, duration: entry.duration });
      assert.equal(loaded.rendererBackend, report.rendererBackend);
      const frameTimes = Array.from({ length: Math.round(end * 30) + 1 }, (_, i) => i / 30);
      // Exclude the endpoint so every encoded frame represents exactly 1/fps.
      for (let index = 0; index < Math.round((end - start) * fps); index++) {
        const frameIndex = Math.round(start * 30) + index * (30 / fps);
        const time = frameTimes[frameIndex];
        const sample = await page.evaluate(({ frameIndex, frameTimes }) =>
          window.noonHostRaster.renderThrough(frameIndex, frameTimes), { frameIndex, frameTimes });
        assert.equal(sample.error, null);
        assert.equal(sample.presented, true);
        assert.equal(sample.rendererBackend, report.rendererBackend);
        assertCaptureTime(entry, sample, time);
        const png = await page.locator("#scene").screenshot();
        const decoded = pngjs.PNG.sync.read(png);
        assert.equal(decoded.width, report.viewport.width);
        assert.equal(decoded.height, report.viewport.height);
        samples.push({ requestedTime: time, publishedTime: sample.publishedTime, pngSha256: hash(png) });
        await writeFile(path.join(frames, `${String(index).padStart(4, "0")}.png`), png);
      }
      assert.ok(new Set(samples.map(sample => sample.pngSha256)).size >= 4, `${id}: static capture`);
      assert.deepEqual(errors, [], `${id}: browser errors`);
      const input = ["-v", "error", "-y", "-framerate", String(fps), "-i", path.join(frames, "%04d.png")];
      execFileSync("ffmpeg", [...input, "-vf", "fps=10,scale=480:-2:flags=lanczos,split[a][b];[a]palettegen=max_colors=128[p];[b][p]paletteuse=dither=bayer:bayer_scale=3", "-loop", "0", path.join(scratch, `${id}.gif`)], { timeout: 120_000 });
      execFileSync("ffmpeg", [...input, "-c:v", "libx264", "-crf", "23", "-pix_fmt", "yuv420p", "-movflags", "+faststart", path.join(scratch, `${id}.mp4`)], { timeout: 120_000 });
      const files = {};
      for (const extension of ["gif", "mp4"]) {
        const bytes = await readFile(path.join(scratch, `${id}.${extension}`));
        assert.ok(bytes.length < 6_000_000, `${id}.${extension}: preview exceeds size budget`);
        files[extension] = { bytes: bytes.length, sha256: hash(bytes) };
      }
      report.results.push({ id, source: entry.path, sourceSha256: hash(source), start, end, samples, files });
      console.log(`Captured ${id}: ${samples.length} actual frames`);
    } catch (error) {
      const evidence = path.join(root, "browser-smoke-artifacts/project-site");
      await mkdir(evidence, { recursive: true });
      await writeFile(path.join(evidence, `${id}-failure.json`), JSON.stringify({
        id, error: String(error.stack ?? error), runtime: report.runtime,
        sourceSha256: hash(source), samples, pageErrors: errors,
      }, null, 2));
      await page.screenshot({ path: path.join(evidence, `${id}-failure.png`) }).catch(() => {});
      throw error;
    } finally {
      await page.close();
    }
  }
  await context.close();
  // Publish as a bundle only after every selection has passed.
  await mkdir(output, { recursive: true });
  for (const { id } of selections) for (const extension of ["gif", "mp4"]) {
    await writeFile(path.join(output, `${id}.${extension}`), await readFile(path.join(scratch, `${id}.${extension}`)));
  }
  await writeFile(path.join(output, "provenance.json"), `${JSON.stringify(report, null, 2)}\n`);
} finally {
  await browser?.close();
  await server.close();
  await rm(scratch, { recursive: true, force: true });
}
