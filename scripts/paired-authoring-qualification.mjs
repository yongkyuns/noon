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
import { browserArgs, rasterFixtureSource } from "./manim-raster-support.mjs";

export async function qualifyPairedAuthoring({ cases, artifactDirectory, port = 0, qualifyLifecycle }) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const output = path.resolve(root, artifactDirectory);
  const selectedIds = process.env.NOON_PAIRED_CASES?.split(",").map(id => id.trim()).filter(Boolean);
  if (selectedIds) {
    for (const id of selectedIds) assert.ok(cases.some(fixture => fixture.id === id), `unknown paired case: ${id}`);
    cases = cases.filter(fixture => selectedIds.includes(fixture.id));
  }
  const fixtures = await Promise.all(cases.map(async fixture => {
    const rawSource = await readFile(path.join(root, fixture.sourcePath ?? `web/python/examples/${fixture.file}`), "utf8");
    const source = fixture.scene ? rasterFixtureSource(rawSource, fixture.scene) : rawSource;
    return { ...fixture, source, sourceHash: createHash("sha256").update(source).digest("hex") };
  }));
  const server = await serveRepository(root, port);
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
      const metrics = await page.evaluate(async ({ label, source, factory, factoryArgs, preparation, duration, sampleTime, playback, boundaries }) => {
        if (label === "python") {
          if (duration > 0) {
            await window.noonHostRaster.load(source, duration);
            const times = sampleTime > 0 ? [0, sampleTime] : [0];
            return window.noonHostRaster.renderThrough(times.length - 1, times);
          }
          const { PythonAuthoringClient } = await import("./authoring-client.js");
          const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
          const client = new PythonAuthoringClient();
          const failures = [];
          const execution = new AuthoringExecutionClient(document.querySelector("#scene"), {
            onError: error => failures.push(String(error)),
          });
          window.pairedAuthoring = client;
          window.pairedExecution = execution;
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
          throw new Error("static Python authoring context did not present");
        }
        const wasm = await import("./pkg/noon_web.js");
        await wasm.default();
        const canvas = document.querySelector("#scene");
        const args = [...factoryArgs];
        if (preparation) {
          const module = await import(preparation.module);
          const prepared = await module[preparation.export]();
          args.push(preparation.wrapper ? new wasm[preparation.wrapper](prepared) : prepared);
        }
        const renderer = await wasm[factory](canvas.transferControlToOffscreen(), ...args);
        window.pairedRenderer = renderer;
        const present = async () => {
          for (let attempt = 0; attempt < 60; attempt++) {
            if (renderer.render()) return true;
            await new Promise(resolve => requestAnimationFrame(resolve));
          }
          return false;
        };
        // Direct hosts must consume the initial publication before a seek.
        let presented = await present();
        if (presented && playback === "live") {
          const { sampleDirectProgram } = await import("../scripts/direct-program-sample.mjs");
          const times = [...new Set([0, ...boundaries.filter(time => time < sampleTime), sampleTime])]
            .sort((a, b) => a - b);
          for (const time of times) await sampleDirectProgram(renderer, time);
        } else if (presented && sampleTime > 0) {
          renderer.seekDirect(sampleTime);
          presented = await present();
        }
        return { presented, time: renderer.time(), objectCount: renderer.objectCount(),
          drawCalls: renderer.lastDrawCalls(), rendererBackend: renderer.rendererBackend() };
      }, { label, source: fixture.source, factory: fixture.factory, factoryArgs: fixture.factoryArgs ?? [],
        preparation: fixture.preparation, playback: fixture.playback, boundaries: fixture.boundaries ?? [],
        duration: fixture.duration ?? 0, sampleTime: fixture.sampleTime ?? 0 });
      assert.deepEqual(errors, []);
      assert.equal(metrics.presented, true);
      assert.ok(Math.abs(metrics.time - (fixture.sampleTime ?? 0)) < 1e-6, `${fixture.id}/${label}: sample time ${metrics.time} differs from ${fixture.sampleTime ?? 0}`);
      assert.ok(metrics.objectCount > 0, "paired scenes must contain render objects");
      if (fixture.objectCount !== undefined) assert.equal(metrics.objectCount, fixture.objectCount);
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
          console.log(`[PASS] ${fixture.id}/${expectedBackend}: rust-wasm host`);
          const python = await capture(context, "python", expectedBackend, fixture);
          console.log(`[PASS] ${fixture.id}/${expectedBackend}: Python host`);
          assert.equal(python.metrics.objectCount, rust.metrics.objectCount, "paired hosts must publish the same object count");
          assert.equal(rust.png.width, 960);
          assert.equal(rust.png.height, 540);
          assert.equal(python.png.width, rust.png.width);
          assert.equal(python.png.height, rust.png.height);
          let foregroundPixels = 0;
          let differingPixels = 0;
          for (let i = 0; i < rust.png.data.length; i += 4) {
            if (rust.png.data[i] + rust.png.data[i + 1] + rust.png.data[i + 2] > 30) foregroundPixels++;
            if (rust.png.data[i] !== python.png.data[i] || rust.png.data[i + 1] !== python.png.data[i + 1] ||
                rust.png.data[i + 2] !== python.png.data[i + 2] || rust.png.data[i + 3] !== python.png.data[i + 3]) differingPixels++;
          }
          assert.ok(foregroundPixels > 1000, "blank images cannot qualify paired authoring");
          result.static[fixture.id] = { rust: rust.metrics, python: python.metrics, foregroundPixels, differingPixels };
          assert.equal(differingPixels, 0, "Rust/WASM and Python must produce identical pixels on the same backend");
          console.log(`[PASS] ${fixture.id}/${expectedBackend}: paired pixels identical`);
        }

        if (qualifyLifecycle) {
          result.lifecycle = await qualifyLifecycle(context, server.baseUrl, expectedBackend);
          console.log(`[PASS] ${expectedBackend}: lifecycle checks`);
        }
        result.passed = true;
        console.log(`[PASS] ${expectedBackend}: paired pixels identical${qualifyLifecycle ? "; lifecycle checks passed" : ""}`);
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
  assert.deepEqual(failures, [], `paired qualification failed; see ${output}/report.json`);

  return report;
}
