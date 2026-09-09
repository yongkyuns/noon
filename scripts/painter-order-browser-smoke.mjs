import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.NOON_PAINTER_ORDER_PORT ?? "4197");
const source = await readFile(path.join(repoRoot, "web/python/examples/painter_order_overlap.py"), "utf8");
const server = await serveRepository(repoRoot, port, { crossOriginIsolated: true });
let browser;
try {
  browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs("webgpu") });
  for (const label of ["Rust", "Python"]) {
    const page = await browser.newPage({ viewport: { width: 1000, height: 600 } });
    const errors = [];
    page.on("pageerror", error => errors.push(String(error)));
    page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
    await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });
    const metrics = await page.evaluate(async ({ label, source }) => {
      const canvas = document.querySelector("#scene");
      canvas.width = 960; canvas.height = 540;
      canvas.style.width = "960px"; canvas.style.height = "540px";
      if (label === "Rust") {
        const wasm = await import("./pkg/noon_web.js");
        await wasm.default();
        const renderer = await wasm.createDirectPainterOrderSmokeRenderer(canvas.transferControlToOffscreen());
        window.painterRenderer = renderer;
        async function presentFrame() {
          for (let attempt = 0; attempt < 60; attempt++) {
            if (renderer.render()) return;
            await new Promise(resolve => requestAnimationFrame(resolve));
          }
          throw new Error("direct painter-order frame was not presented");
        }
        // Admit the bootstrap publication before requesting another runtime frame.
        await presentFrame();
        renderer.seekDirect(0.5);
        await presentFrame();
        return { objectCount: renderer.objectCount(), drawCalls: renderer.lastDrawCalls(),
          rendererBackend: renderer.rendererBackend(), time: renderer.time() };
      }
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
      const authoring = new PythonAuthoringClient();
      const execution = new AuthoringExecutionClient(canvas);
      window.painterExecution = execution; window.painterAuthoring = authoring;
      const authored = await authoring.run(source, {});
      if (!authored.semanticExecution || authored.duration !== 1) throw new Error("missing shared painter-order execution");
      await execution.startSemanticExecution(authored.semanticExecution, {
        authoringClient: authoring, initiallyPaused: true, transportMode: "transferable",
        loopDurationSeconds: authored.duration,
      });
      await execution.pause();
      const before = (await execution.metrics()).metrics.presentedFrames;
      await execution.seek(0.5);
      let metrics;
      for (let attempt = 0; attempt < 60; attempt++) {
        metrics = (await execution.metrics()).metrics;
        if (metrics.presentedFrames > before) break;
        await new Promise(resolve => requestAnimationFrame(resolve));
      }
      if (metrics.presentedFrames <= before) throw new Error("Python painter-order seek was not presented");
      return { ...metrics, rendererBackend: execution.rendererBackend };
    }, { label, source });
    assert.equal(metrics.rendererBackend, "WebGPU", `${label} must exercise WebGPU`);
    assert.equal(metrics.objectCount, 3, `${label} must retain all three objects`);
    assert.ok(Math.abs(metrics.time - 0.5) < 1e-6, `${label} must sample the animation midpoint`);
    assert.ok(metrics.drawCalls >= 3, `${label} must cross renderer pipelines`);
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => resolve())));
    const screenshot = await page.locator("#scene").screenshot();
    const png = PNG.sync.read(screenshot);
    const centerX = Math.floor(png.width / 2);
    const centerY = Math.floor(png.height / 2);
    const samples = [];
    for (let dy = -2; dy <= 2; dy += 1) {
      for (let dx = -2; dx <= 2; dx += 1) {
        const offset = ((centerY + dy) * png.width + centerX + dx) * 4;
        samples.push([png.data[offset], png.data[offset + 1], png.data[offset + 2]]);
      }
    }
    const mean = samples.reduce(
      (sum, rgb) => [sum[0] + rgb[0], sum[1] + rgb[1], sum[2] + rgb[2]],
      [0, 0, 0],
    ).map((value) => value / samples.length);
    const [red, green, blue] = mean;
    assert.ok(
      green > red + 25 && green > blue + 25,
      `center pixel is not green-top painter order: rgb=${mean.map((value) => value.toFixed(1)).join(",")}`,
    );
    console.log(
      `${label} WebGPU painter-order pixel oracle passed: center rgb=${mean.map((value) => value.toFixed(1)).join(",")}`,
    );
    assert.deepEqual(errors, [], `${label} browser errors`);
    await page.evaluate(() => {
      window.painterExecution?.terminate();
      window.painterAuthoring?.terminate();
      window.painterRenderer?.free();
    });
    await page.close();
  }
} finally {
  await browser?.close();
  await server.close();
}

// Keep a compact sustained timestamp-query regression in the existing WebGPU
// browser gate. This is a correctness/lifetime oracle, not a performance result,
// so pin its headless Linux backend to the same deterministic SwiftShader/Vulkan
// stack used by the dedicated WebGPU recovery tests. Normal perf-profile runs
// remain hardware-selected unless the caller explicitly requests software WebGPU.
const sustained = spawnSync(process.execPath, ["scripts/perf-profile.mjs"], {
  cwd: repoRoot,
  encoding: "utf8",
  env: {
    ...process.env,
    NOON_PERF_BACKEND: "webgpu",
    NOON_PERF_FORCE_SOFTWARE_WEBGPU: "1",
    NOON_PERF_REQUIRE_GPU_TIMESTAMPS: "1",
    NOON_PERF_COUNTS: "10000",
    NOON_PERF_LAYOUTS: "fixed",
    NOON_PERF_WARMUP: "8",
    NOON_PERF_FRAMES: "64",
    NOON_PERF_PORT: "4188",
  },
});
if (sustained.status !== 0) {
  throw new Error(
    `Sustained WebGPU timestamp profiling failed:\n${sustained.stdout}\n${sustained.stderr}`,
  );
}
console.log("Sustained WebGPU timestamp profiler regression passed (64 measured frames)");
