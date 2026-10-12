import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import pngjs from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs, rasterFixtureSource } from "./manim-raster-support.mjs";
import { compareTipRoi, enforceTipMetrics, TIP_LIMITS } from "./arrow-tip-raster-metrics.mjs";

const { PNG } = pngjs;
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(root, process.env.NOON_ARROW_TIP_ARTIFACTS ?? "ci-artifacts/arrow-tip");
const source = await readFile(path.join(root, "parity/manim-v0.21/core-examples/arrow_tip_regression.py"), "utf8");
const reference = JSON.parse(await readFile(path.join(output, "reference.json"), "utf8"));
assert.equal(reference.manim_version, "0.21.0");
assert.equal(reference.renderer, "cairo");
assert.equal(reference.source_sha256, createHash("sha256").update(source).digest("hex"), "oracle source must match tested source");
assert.deepEqual(reference.runs.map((r) => r.dpr), [1, 2], "both physical pixel densities are mandatory");
const report = {
  checkout: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  source_sha256: reference.source_sha256, reference: "ManimCE 0.21.0 / Cairo",
  limits: TIP_LIMITS, captures: [], failures: [],
};

function crop(image, roi) {
  const result = new PNG({ width: roi.width, height: roi.height });
  PNG.bitblt(image, result, roi.x, roi.y, roi.width, roi.height, 0, 0);
  return result;
}

async function saveTipComparison(expected, actual, roi, directory) {
  await mkdir(directory, { recursive: true });
  const a = crop(expected, roi);
  const b = crop(actual, roi);
  const diff = new PNG({ width: roi.width, height: roi.height });
  for (let i = 0; i < diff.data.length; i += 4) {
    for (let channel = 0; channel < 3; channel += 1) diff.data[i + channel] = Math.min(255, Math.abs(a.data[i + channel] - b.data[i + channel]) * 4);
    diff.data[i + 3] = 255;
  }
  await Promise.all([["manim", a], ["noon", b], ["diff-x4", diff]].map(([name, image]) =>
    writeFile(path.join(directory, `${name}.png`), PNG.sync.write(image))));
}

async function bounded(operation) {
  let timer;
  try {
    return await Promise.race([operation(), new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error("arrow-tip browser case exceeded 180 seconds")), 180_000);
    })]);
  } finally {
    clearTimeout(timer);
  }
}

const server = await serveRepository(root, Number(process.env.NOON_ARROW_TIP_PORT ?? 4197));
try {
  for (const backend of ["webgpu", "webgl"]) {
    let browser;
    try {
      browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs(backend) });
      for (const run of reference.runs) {
        const label = `${backend}-dpr${run.dpr}`;
        const context = await browser.newContext({ viewport: { width: 1000, height: 580 }, deviceScaleFactor: run.dpr });
        const page = await context.newPage();
        try {
          await bounded(async () => {
            await page.goto(`${server.baseUrl}/web/manim-raster-host.html`, { waitUntil: "load" });
            await page.waitForFunction(() => window.noonHostRaster);
            const loaded = await page.evaluate((source) => window.noonHostRaster.load(source, 2),
              rasterFixtureSource(source, "ArrowTipRegression"));
            assert.equal(loaded.kind, "semantic_execution");
            assert.equal(loaded.rendererBackend, backend === "webgpu" ? "WebGPU" : "WebGL2");
            const times = [...run.frames.map((frame) => frame.time), run.duration];
            assert.equal(run.samples.length, 6, `${label}: required static/motion checkpoints`);
            for (const index of run.samples) {
              const frame = run.frames[index];
              assert.equal(frame.rois.length, 18, `${label}: all arrow tips`);
              const metrics = await page.evaluate(({ index, times }) => window.noonHostRaster.renderThrough(index, times), { index, times });
              assert.equal(metrics.error, null);
              assert.equal(metrics.presented, true);
              assert.ok(Math.abs(metrics.time - frame.time) < 1e-9, "exact oracle logical time");
              const backing = await page.locator("#scene").evaluate((canvas) => ({ width: canvas.width, height: canvas.height }));
              assert.deepEqual(backing, { width: run.width, height: run.height }, "native canvas pixels, not CSS-upscaled output");
              const directory = path.join(output, label, `frame-${String(index).padStart(4, "0")}`);
              await mkdir(directory, { recursive: true });
              const buffer = await page.locator("#scene").screenshot({ path: path.join(directory, "noon.png"), scale: "device" });
              const actual = PNG.sync.read(buffer);
              const expected = PNG.sync.read(await readFile(path.join(output, run.directory, frame.image)));
              const capture = { label, index, time: frame.time, backing, metrics, tips: [] };
              for (const roi of frame.rois) {
                const tipLabel = `${label}/frame-${index}/${roi.name}`;
                try {
                  const comparison = compareTipRoi(expected, actual, roi);
                  capture.tips.push({ name: roi.name, roi, ...comparison });
                  enforceTipMetrics(comparison, tipLabel);
                } catch (error) {
                  report.failures.push({ label: tipLabel, error: String(error) });
                }
                await saveTipComparison(expected, actual, roi, path.join(directory, roi.name));
              }
              report.captures.push(capture);
            }
            const completed = await page.evaluate((times) => window.noonHostRaster.renderThrough(times.length - 1, times), times);
            assert.equal(completed.authoredDuration, run.duration);
            assert.equal(completed.error, null);
          });
        } catch (error) {
          report.failures.push({ label, error: String(error.stack ?? error) });
        } finally {
          await context.close();
        }
      }
    } catch (error) {
      report.failures.push({ label: backend, error: String(error.stack ?? error) });
    } finally {
      await browser?.close();
    }
  }
} finally {
  await server.close();
  await writeFile(path.join(output, "comparison.json"), `${JSON.stringify(report, null, 2)}\n`);
}
assert.equal(report.captures.length, 24, "all backend/DPR/checkpoint captures must execute");
assert.equal(report.failures.length, 0, JSON.stringify(report.failures, null, 2));
console.log("PASS: 432 tip-local comparisons, WebGPU/WebGL2, DPR 1/2, six static/motion checkpoints");
