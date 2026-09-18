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
const fixtures = await Promise.all([
  { id: "coordinates", file: "coordinate_plotting.py", factory: "createCoordinatePlottingRenderer", objectCount: 30 },
  { id: "number-plane", file: "number_plane.py", factory: "createNumberPlaneRenderer", objectCount: 24 },
  { id: "implicit", file: "implicit_plotting.py", factory: "createImplicitPlottingRenderer", objectCount: 16 },
].map(async fixture => {
  const source = await readFile(path.join(root, "web/python/examples", fixture.file), "utf8");
  return { ...fixture, source, sourceHash: createHash("sha256").update(source).digest("hex") };
}));
const server = await serveRepository(root, 4199);
await mkdir(output, { recursive: true });
const report = { fixtures: fixtures.map(({ id, sourceHash }) => ({ id, sourceHash })), backends: [] };
const failures = [];

async function capture(context, label, expectedBackend, fixture) {
  const page = await context.newPage();
  page.setDefaultTimeout(90_000);
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  try {
    await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
    await page.waitForFunction(() => window.noonHostRaster);
    const metrics = await page.evaluate(async ({ label, source, factory }) => {
      if (label === "python") {
        const { PythonAuthoringClient } = await import("./authoring-client.js");
        const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
        const client = new PythonAuthoringClient();
        const failures = [];
        const execution = new AuthoringExecutionClient(document.querySelector("#scene"), {
          onError: error => failures.push(String(error)),
        });
        window.plottingAuthoring = client;
        window.plottingExecution = execution;
        const result = await client.run(source);
        if (result.duration !== 0 || !result.semanticExecution ||
            result.semanticExecution.continuationGeneration !== undefined) {
          throw new Error("static example must return a context without inventing a continuation");
        }
        await execution.startSemanticExecution(result.semanticExecution, {
          authoringClient: client, initiallyPaused: true, transportMode: "transferable",
        });
        await execution.advanceTo(0);
        for (let attempt = 0; attempt < 100; attempt++) {
          if (failures.length) throw new Error(failures.join("; "));
          const { metrics } = await execution.metrics();
          if (metrics.ready && metrics.retained && metrics.presentedFrames > 0) {
            return { presented: true, time: metrics.time, objectCount: metrics.objectCount,
              drawCalls: metrics.drawCalls, rendererBackend: metrics.backend };
          }
          await new Promise(resolve => setTimeout(resolve, 20));
        }
        throw new Error("static Python plotting context did not present");
      }
      const wasm = await import("./pkg/noon_web.js");
      await wasm.default();
      const canvas = document.querySelector("#scene");
      const renderer = await wasm[factory](canvas.transferControlToOffscreen());
      window.plottingRenderer = renderer;
      let presented = false;
      for (let attempt = 0; attempt < 60 && !presented; attempt++) {
        presented = renderer.render();
        if (!presented) await new Promise(resolve => requestAnimationFrame(resolve));
      }
      return { presented, time: renderer.time(), objectCount: renderer.objectCount(),
        drawCalls: renderer.lastDrawCalls(), rendererBackend: renderer.rendererBackend() };
    }, { label, source: fixture.source, factory: fixture.factory });
    assert.deepEqual(errors, []);
    assert.equal(metrics.presented, true);
    assert.equal(metrics.time, 0);
    assert.equal(metrics.objectCount, fixture.objectCount);
    assert.equal(metrics.rendererBackend, expectedBackend);
    assert.ok(metrics.drawCalls > 0);
    await page.evaluate(() => new Promise(resolve =>
      requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const pixels = await page.locator("#scene").screenshot({
      path: path.join(output, `${fixture.id}-${expectedBackend}-${label}.png`),
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
      result.static = {};
      for (const fixture of fixtures) {
        const rust = await capture(context, "rust-wasm", expectedBackend, fixture);
        const python = await capture(context, "python", expectedBackend, fixture);
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
        result.static[fixture.id] = { rust: rust.metrics, python: python.metrics, foregroundPixels, differingPixels };
        assert.equal(differingPixels, 0, "Rust/WASM and Python must produce identical pixels on the same backend");
      }

      // This invokes the actual worker/continuation test including new curves
      // authored after an axes animation, not a Python-only unit fixture.
      const page = await context.newPage();
      page.setDefaultTimeout(90_000);
      await page.goto(`${server.baseUrl}/web/manim-compat-smoke.html`);
      await page.waitForFunction(() => window.noonManimCompat);
      const lifecycle = await page.evaluate(async () => (await window.noonManimCompat.ready()).plotting);
      assert.equal(lifecycle.backend, expectedBackend);
      assert.equal(lifecycle.objectCount, 14);
      assert.ok(Math.abs(lifecycle.duration - 0.8) < 1e-6);
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
