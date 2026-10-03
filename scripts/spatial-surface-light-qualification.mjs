// Real point-lit mesh qualification through direct Rust/WASM and the Python worker.
// The fixture is not a claim of Manim Cairo shading parity.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import pngjs from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const { PNG } = pngjs;
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const backend = process.env.NOON_SPATIAL_SURFACE_BACKEND ?? "webgl";
assert.ok(backend === "webgl" || backend === "webgpu", `unknown backend ${backend}`);
const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
const port = Number(process.env.NOON_SPATIAL_SURFACE_PORT ?? "4213");
const artifacts = path.resolve(process.env.NOON_SPATIAL_SURFACE_ARTIFACTS ??
  path.join(root, "browser-smoke-artifacts/spatial-surface-light", backend));
const source = await readFile(
  path.join(root, "web/python/examples/noon_spatial_surface_lighting.py"), "utf8",
);
const sourceHash = createHash("sha256").update(source).digest("hex");
const server = await serveRepository(root, port);
const browser = await chromium.launch({ headless: true, args: browserArgs(backend) });
const context = await browser.newContext({ viewport: { width: 960, height: 540 }, deviceScaleFactor: 1 });
const samples = [0, 0.5, 1];
const report = { backend, sourceHash, fixture: "spatial-surface-point-light", samples: [] };

async function captureDirect() {
  const page = await context.newPage();
  try {
    await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
    await page.waitForFunction(() => window.noonHostRaster);
    await page.evaluate(async () => {
      const canvas = document.querySelector("#scene");
      const wasm = await import("./pkg/noon_web.js");
      await wasm.default();
      const renderer = await wasm.createDirectSpatialSurfaceLightingSmokeRenderer(
        canvas.transferControlToOffscreen(),
      );
      renderer.resize(960, 540);
      window.spatialDirectRenderer = renderer;
      const { sampleDirectProgram } = await import("../scripts/direct-program-sample.mjs");
      window.sampleSpatialDirect = time => sampleDirectProgram(renderer, time);
    });
    const frames = [];
    for (const time of samples) {
      const frame = await page.evaluate(async time => {
        const metrics = await window.sampleSpatialDirect(time);
        return { metrics, uploads: window.spatialDirectRenderer.lastBytesUploaded(),
          geometryCacheMisses: window.spatialDirectRenderer.lastGeometryCacheMisses() };
      }, time);
      frames.push({ time, ...frame, png: await page.locator("#scene").screenshot() });
    }
    return frames;
  } finally {
    await page.close();
  }
}

async function capturePython(targetContext = context) {
  const page = await targetContext.newPage();
  try {
    await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
    await page.waitForFunction(() => window.noonHostRaster);
    const loaded = await page.evaluate(async ({ source }) => {
      const result = await window.noonHostRaster.load(source, 1);
      return result;
    }, { source });
    assert.equal(loaded.rendererBackend, expectedBackend);
    const frames = [];
    const frameTimes = [...samples];
    for (let frameIndex = 0; frameIndex < frameTimes.length; frameIndex += 1) {
      const time = frameTimes[frameIndex];
      const sample = await page.evaluate(({ frameIndex, frameTimes }) =>
        window.noonHostRaster.renderThrough(frameIndex, frameTimes, {
          stopAtSourceCompletion: frameIndex === frameTimes.length - 1,
        }), { frameIndex, frameTimes });
      assert.equal(sample.time, time);
      if (frameIndex === frameTimes.length - 1) {
        assert.equal(sample.authoredDuration, 1);
        assert.equal(sample.sourceCompleted, true);
      }
      frames.push({ time, sample, png: await page.locator("#scene").screenshot() });
    }
    return frames;
  } finally {
    await page.evaluate(() => window.noonHostRaster.close()).catch(() => {});
    await page.close();
  }
}

async function assertMissingLightRejected() {
  const page = await context.newPage();
  try {
    await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
    await page.waitForFunction(() => window.noonHostRaster);
    const missingLightSource = `from noon import *
class MissingPointLight(SpatialScene):
    def construct(self):
        mesh = Mesh3D.parametric(lambda u, v: (u, v, 0.25*u*v),
            u_range=(-1.5, 1.5), v_range=(-1.5, 1.5), resolution=(8, 8), point_lit=True)
        self.add(mesh)
`;
    const error = await Promise.race([
      page.evaluate(async code => {
        try {
          await window.noonHostRaster.load(code, 0.1);
          await window.noonHostRaster.renderThrough(0, [0]);
          return null;
        } catch (failure) { return String(failure); }
      }, missingLightSource),
      new Promise((_, reject) => setTimeout(() => reject(new Error("missing-light check timed out")), 15_000)),
    ]);
    assert.match(error ?? "", /MissingPointLight|point light|spatial renderer publication/i,
      "point-lit geometry without a light must be rejected by the renderer contract");
  } finally {
    await page.evaluate(() => window.noonHostRaster.close()).catch(() => {});
    await page.close();
  }
}

try {
  await mkdir(artifacts, { recursive: true });
  const direct = await captureDirect();
  const python = await capturePython();
  for (let index = 0; index < samples.length; index += 1) {
    const rust = PNG.sync.read(direct[index].png);
    const worker = PNG.sync.read(python[index].png);
    assert.equal(rust.width, 960);
    assert.equal(rust.height, 540);
    assert.equal(worker.width, rust.width);
    assert.equal(worker.height, rust.height);
    assert.equal(direct[index].metrics.backend, expectedBackend);
    assert.equal(python[index].sample.presented, true);
    assert.equal(python[index].sample.rendererBackend, expectedBackend);
    assert.ok(direct[index].metrics.drawCalls > 0);
    assert.ok(python[index].sample.drawCalls > 0);
    let differingChannels = 0;
    let maxChannelDelta = 0;
    let totalDelta = 0;
    for (let channel = 0; channel < rust.data.length; channel += 1) {
      const delta = Math.abs(rust.data[channel] - worker.data[channel]);
      if (delta !== 0) differingChannels += 1;
      maxChannelDelta = Math.max(maxChannelDelta, delta);
      totalDelta += delta;
    }
    assert.ok(maxChannelDelta <= 1,
      `direct/worker surface pixels differ by more than one channel LSB at t=${samples[index]}`);
    const meanDelta = totalDelta / rust.data.length;
    assert.ok(meanDelta <= 0.001, `direct/worker surface pixel delta ${meanDelta} at t=${samples[index]}`);
    if (index > 0) {
      const previousDirect = PNG.sync.read(direct[index - 1].png).data;
      const previousPython = PNG.sync.read(python[index - 1].png).data;
      assert.notDeepEqual(rust.data, previousDirect, "moving the light must alter direct pixels");
      assert.notDeepEqual(worker.data, previousPython, "moving the light must alter Python-worker pixels");
    }
    const filename = `point-lit-${samples[index]}.png`;
    await writeFile(path.join(artifacts, `direct-${filename}`), direct[index].png);
    await writeFile(path.join(artifacts, `python-${filename}`), python[index].png);
    report.samples.push({
      time: samples[index],
      directMetrics: direct[index].metrics,
      pythonMetrics: python[index].sample,
      directBytesUploaded: direct[index].uploads,
      directGeometryCacheMisses: direct[index].geometryCacheMisses,
      differingChannels,
      maxChannelDelta,
      meanDelta,
    });
  }
  for (const frame of direct.slice(1)) {
    assert.equal(frame.uploads, 32, "light-only movement uploads only its compact uniform");
    assert.equal(frame.geometryCacheMisses, 0, "light-only movement retains geometry residency");
  }
  await assertMissingLightRejected();
  report.missingLightRejected = true;
  const freshContext = await browser.newContext({
    viewport: { width: 960, height: 540 }, deviceScaleFactor: 1,
  });
  try {
    const recovered = await capturePython(freshContext);
    const recoveredEnd = PNG.sync.read(recovered.at(-1).png);
    const directEnd = PNG.sync.read(direct.at(-1).png);
    let recoveryTotalDelta = 0;
    let recoveryMaxDelta = 0;
    for (let channel = 0; channel < directEnd.data.length; channel += 1) {
      const delta = Math.abs(recoveredEnd.data[channel] - directEnd.data[channel]);
      recoveryTotalDelta += delta;
      recoveryMaxDelta = Math.max(recoveryMaxDelta, delta);
    }
    assert.ok(recoveryMaxDelta <= 1 && recoveryTotalDelta / directEnd.data.length <= 0.001,
      "a missing-light failure must not poison a fresh Python-worker context");
  } finally {
    await freshContext.close();
  }
  report.freshContextAfterMissingLight = true;
  await writeFile(path.join(artifacts, "report.json"), JSON.stringify(report, null, 2));
} finally {
  await context.close();
  await browser.close();
  await server.close();
}
