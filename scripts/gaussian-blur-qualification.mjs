// Real reusable Rust RGBA raster operator on the normal browser host's device.
// This explicitly qualifies a renderer prerequisite, not Scene/Python Blur.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { referenceRgbaGaussian, referenceRgbaGaussianAt } from "./gaussian-blur-reference.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(root, process.env.NOON_RGBA_BLUR_ARTIFACTS ?? "browser-smoke-artifacts/rgba-gaussian-blur");
await mkdir(output, { recursive: true });
const report = { passed: false, scope: "reusable RGBA raster operator; Scene/Python Blur remains unqualified", backends: [] };
report.runtime = JSON.parse(await readFile(path.join(root, "web/runtime-build-identity.json"), "utf8"));
report.wasmSha256 = createHash("sha256").update(await readFile(path.join(root, "web/pkg/noon_web_bg.wasm"))).digest("hex");
const server = await serveRepository(root, 0);
try {
  for (const backend of ["webgpu", "webgl"]) {
    const browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs(backend) });
    const result = { requestedBackend: backend, browser: browser.version(), samples: [] };
    report.backends.push(result);
    try {
      const page = await browser.newPage();
      page.setDefaultTimeout(90_000);
      const errors = [];
      page.on("pageerror", error => errors.push(String(error)));
      await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
      result.actualBackend = await page.evaluate(async () => {
        const wasm = await import("./pkg/noon_web.js");
        await wasm.default();
        window.blurRenderer = await wasm.createDirectExecutionSmokeRenderer(document.querySelector("#scene").transferControlToOffscreen());
        return window.blurRenderer.rendererBackend();
      });
      assert.equal(result.actualBackend, backend === "webgpu" ? "WebGPU" : "WebGL2");
      for (let kind = 0; kind < 4; kind++) {
        const [width, height] = kind === 3 ? [385, 385] : [33, 29];
        const pixels = new Array(width * height * 4).fill(0);
        for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
          let color = [0, 0, 0, 0];
          if (kind === 0 && x === Math.floor(width / 2) && y === Math.floor(height / 2)) color = [83, 19, 131, 173];
          if (kind === 1) { const alpha = (x * 17 + y * 29) % 256; color = [Math.floor(alpha / 2), Math.floor(alpha / 3), Math.floor(alpha / 5), alpha]; }
          if (kind === 2 && (x === 0 || y === 0)) color = [0, 1, 0, 2];
          if (kind === 3) color = [83, 19, 131, 173];
          pixels.splice((y * width + x) * 4, 4, ...color);
        }
        for (const sigma of kind === 3 ? [64] : [0.8, 3.25, 6]) {
          const samples = await page.evaluate(async ({ pixels, width, height, sigma }) => JSON.parse(
            await window.blurRenderer.qualifyGaussianBlurRaster(Uint8Array.from(pixels), width, height, sigma)), { pixels, width, height, sigma });
          assert.equal(samples.length, 2);
          for (let index = 0; index < samples.length; index++) {
            const sample = samples[index], opacity = index === 0 ? 1 : 0.37;
            assert.ok(Math.abs(sample.opacity - opacity) < 1e-6);
            assert.equal(sample.pixels.length, width * height * 4);
            const expected = kind === 3 ? [[192,192], [0,0], [384,384], [192,0], [0,192]].flatMap(([x,y]) =>
              referenceRgbaGaussianAt(pixels, width, height, sigma, x, y, opacity).map((value, channel) => [(y*width+x)*4+channel, Math.round(value*255)]))
              : referenceRgbaGaussian(pixels, width, height, sigma, opacity).map((v, at) => [at, Math.round(v * 255)]);
            const maxByteError = expected.reduce((maximum, [at,value]) => Math.max(maximum, Math.abs(value - sample.pixels[at])), 0);
            assert.ok(maxByteError <= 2, `${backend} kind=${kind} sigma=${sigma} opacity=${opacity}: ${maxByteError}`);
            assert.equal(sample.pipelineCompiles, index === 0 ? 3 : 0);
            assert.equal(sample.textureAllocations, index === 0 ? 7 : 0);
            assert.equal(sample.bytesUploaded, index === 0 ? 1056 : 16);
            assert.equal(sample.blurPasses, index === 0 ? 2 : 0);
            assert.equal(sample.outputPasses, 1);
            assert.equal(sample.scratchBytes, width * height * 28);
            for (const key of ["cleanBytesUploaded", "cleanTextureAllocations", "cleanBlurPasses", "cleanOutputPasses"]) assert.equal(sample[key], 0);
            result.samples.push({ kind, sigma, width, height, ...sample, comparedChannels: expected.length, maxByteError });
          }
          process.stdout.write(`[PASS] ${result.actualBackend} RGBA Gaussian kind=${kind} sigma=${sigma}\n`);
        }
      }
      assert.deepEqual(errors, []);
      await page.evaluate(() => window.blurRenderer.free());
      result.passed = true;
    } finally { await browser.close(); }
  }
  report.passed = true;
} catch (error) {
  report.error = String(error.stack ?? error);
  throw error;
} finally {
  await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2));
  await server.close();
}
