// Paired pixels use the shared native/direct-Rust builder and the real Pyodide
// authoring worker. No mock bridge, alternate renderer or serialized Rust scene.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(root, "plotting-artifacts");
const source = await readFile(path.join(root, "web/python/examples/coordinate_plotting.py"), "utf8");
const sourceHash = createHash("sha256").update(source).digest("hex");
const server = await serveRepository(root, 4199);
await mkdir(output, { recursive: true });
const report = { pythonSourceSha256: sourceHash, backends: [] };
const failures = [];

async function capture(context, label, expectedBackend) {
  const page = await context.newPage();
  page.setDefaultTimeout(90_000);
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  try {
    await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
    await page.waitForFunction(() => window.noonHostRaster);
    const metrics = await page.evaluate(async ({ label, source }) => {
      if (label === "python") {
        const loaded = await window.noonHostRaster.load(source, 1);
        if (loaded.duration !== 0) throw new Error("static example must not invent a timeline");
        return window.noonHostRaster.renderThrough(0, [0]);
      }
      const wasm = await import("./pkg/noon_web.js");
      await wasm.default();
      const canvas = document.querySelector("#scene");
      const renderer = await wasm.createCoordinatePlottingRenderer(canvas.transferControlToOffscreen());
      window.plottingRenderer = renderer;
      let presented = false;
      for (let attempt = 0; attempt < 60 && !presented; attempt++) {
        presented = renderer.render();
        if (!presented) await new Promise(resolve => requestAnimationFrame(resolve));
      }
      return { presented, time: renderer.time(), objectCount: renderer.objectCount(),
        drawCalls: renderer.lastDrawCalls(), rendererBackend: renderer.rendererBackend() };
    }, { label, source });
    assert.deepEqual(errors, []);
    assert.equal(metrics.presented, true);
    assert.equal(metrics.time, 0);
    assert.equal(metrics.objectCount, 17);
    assert.equal(metrics.rendererBackend, expectedBackend);
    assert.ok(metrics.drawCalls > 0);
    await page.evaluate(() => new Promise(resolve =>
      requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const pixels = await page.locator("#scene").screenshot({
      path: path.join(output, `${expectedBackend}-${label}.png`),
    });
    return { metrics, png: PNG.sync.read(pixels) };
  } finally {
    await page.close();
  }
}

try {
  for (const backend of ["webgpu", "webgl"]) {
    const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
    const result = { backend: expectedBackend };
    report.backends.push(result);
    const browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs(backend) });
    try {
      const context = await browser.newContext({ viewport: { width: 1000, height: 600 } });
      const rust = await capture(context, "rust-wasm", expectedBackend);
      const python = await capture(context, "python", expectedBackend);
      assert.equal(rust.png.width, 960);
      assert.equal(rust.png.height, 540);
      assert.equal(python.png.width, rust.png.width);
      assert.equal(python.png.height, rust.png.height);
      const foregroundPixels = Array.from({ length: rust.png.width * rust.png.height }, (_, i) => i * 4)
        .filter(i => rust.png.data[i] + rust.png.data[i + 1] + rust.png.data[i + 2] > 30).length;
      assert.ok(foregroundPixels > 1000, "blank images cannot qualify plotting");
      let differingPixels = 0;
      for (let i = 0; i < rust.png.data.length; i += 4) {
        if (!rust.png.data.subarray(i, i + 4).equals(python.png.data.subarray(i, i + 4))) differingPixels++;
      }
      result.static = { rust: rust.metrics, python: python.metrics, foregroundPixels, differingPixels };
      assert.equal(differingPixels, 0, "Rust/WASM and Python must produce identical pixels on the same backend");

      // This invokes the actual worker/continuation test including new curves
      // authored after an axes animation, not a Python-only unit fixture.
      const page = await context.newPage();
      page.setDefaultTimeout(90_000);
      await page.goto(`${server.baseUrl}/web/manim-compat-smoke.html`);
      await page.waitForFunction(() => window.noonManimCompat);
      const lifecycle = await page.evaluate(async () => (await window.noonManimCompat.ready()).plotting);
      assert.equal(lifecycle.backend, expectedBackend);
      assert.equal(lifecycle.objectCount, 14);
      assert.ok(Math.abs(lifecycle.duration - 0.6) < 1e-6);
      assert.ok(lifecycle.presentedFrames > 0);
      result.lifecycle = lifecycle;
      await page.close();
      result.passed = true;
      console.log(`[PASS] ${expectedBackend}: paired pixels identical; live plotting assertions passed`);
    } catch (error) {
      result.passed = false;
      result.error = error.stack ?? String(error);
      failures.push(result.error);
      console.error(result.error);
    } finally {
      await browser.close();
    }
  }
} finally {
  await writeFile(path.join(output, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
  await server.close();
}
assert.deepEqual(failures, [], "plotting qualification failed; see plotting-artifacts/report.json");
