// Diagnostic only: exact original production cold Noon playback backend selection.
// Four predetermined Chromium flag cases, zero scored product/host performance.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { pathToFileURL, fileURLToPath } from "node:url";

export const STUDY_ID = "1933-macos-product-webgl-select-20261009-01";
export const FIXED_CASES = [
  { id: "original-hardware-webgl-flags", disableFeatures: "WebGPU", addArgs: [] },
  { id: "webgpu-service-off", disableFeatures: "WebGPU,WebGPUService,WebGPUBlobCache", addArgs: [] },
  { id: "service-and-blink-webgpu-off", disableFeatures: "WebGPU,WebGPUService,WebGPUBlobCache", addArgs: ["--disable-blink-features=WebGPU"] },
  { id: "service-and-legacy-switch", disableFeatures: "WebGPU,WebGPUService,WebGPUBlobCache", addArgs: ["--disable-webgpu"] },
];

export function validatePlan(p) {
  assert.equal(p.schema, 1);
  assert.equal(p.studyId, STUDY_ID);
  assert.equal(p.diagnosticOnly, true);
  assert.equal(p.qualification, false);
  assert.equal(p.performanceAcceptance, false);
  assert.equal(p.mergeApproval, false);
  assert.equal(p.runner, "macos-15");
  assert.equal(p.node, "22.23.3");
  assert.equal(p.playwright, "1.62.1");
  assert.equal(p.chromium, "151.0.7922.34");
  assert.deepEqual(p.cases, FIXED_CASES);
  assert.equal(p.source.originalHarness, "f5ec15c7a1a9a70c368e141abfc740f5081efc4e");
  assert.equal(p.source.baselineSource, "4d8d61646cabb1c8c4e47c72bd3d7e77802d6c0f");
  assert.equal(p.source.baselineArtifact, 11523896652);
  assert.equal(p.source.baselineArchiveSha256, "23eb7282da981e6272c18b2ba59a9ff8863d45406cd00efae73fc87657d54071");
  assert.equal(p.priorFailedProduct.completedPairs, 0);
  assert.equal(p.noScoring, true);
  return p;
}

export function flagsFor(mode, originalArgs) {
  const old = "--disable-features=WebGPU";
  assert.equal(originalArgs.filter(s => s.startsWith("--disable-features=")).length, 1);
  assert.equal(originalArgs.filter(s => s === old).length, 1);
  const args = originalArgs.map(x =>
    x === old ? "--disable-features=" + mode.disableFeatures : x);
  return [...args, ...mode.addArgs];
}

export function classifyCase(observed) {
  const blockers = [];
  if (observed?.browserVersion !== "151.0.7922.34") blockers.push("Chrome revision changed");
  if (observed?.status?.patch !== "applied") blockers.push("original Noon cold authored playback failed");
  if (observed?.status?.backend !== "WebGL2") blockers.push("actual Noon renderer backend is not WebGL2");
  if (!/Apple.*Metal Renderer.*Apple Paravirtual/i.test(observed?.gpu?.unmaskedRenderer ?? "")) {
    blockers.push("real Apple Paravirtual Metal WebGL2 renderer not observed");
  }
  if (/swiftshader|llvmpipe|software/i.test(observed?.gpu?.unmaskedRenderer ?? "")) {
    blockers.push("software renderer fallback");
  }
  const rgba = observed?.gpu?.pixel;
  if (!Array.isArray(rgba) || rgba.length !== 4 ||
    [51, 102, 153, 255].some((x, i) => Math.abs(x - rgba[i]) > 3)) {
    blockers.push("real WebGL2 readPixels check failed");
  }
  if (observed?.gpu?.glError !== 0 || observed?.gpu?.contextLost !== false) {
    blockers.push("GL error/context lost");
  }
  if ((observed?.pageErrors?.length ?? 1) !== 0) blockers.push("browser page error");
  return { status: blockers.length ? "not-confirmed" : "eligible-for-fresh-aa-only",
    blockers, qualification: false, performanceAcceptance: false, mergeApproval: false };
}

async function gpuObservation(page) {
  return page.evaluate(async () => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 64;
    const gl = canvas.getContext("webgl2", {
      preserveDrawingBuffer: true, antialias: false, powerPreference: "high-performance",
    });
    if (!gl) return { backend: "missing", unmaskedRenderer: "", pixel: null };
    const dbg = gl.getExtension("WEBGL_debug_renderer_info");
    const unmaskedRenderer = dbg ? String(gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL)) : "";
    const unmaskedVendor = dbg ? String(gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL)) : "";
    gl.viewport(0, 0, 64, 64);
    gl.clearColor(0.2, 0.4, 0.6, 1); gl.clear(gl.COLOR_BUFFER_BIT);
    const p = new Uint8Array(4);
    gl.readPixels(32, 32, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, p);
    const navigatorGpu = typeof navigator.gpu?.requestAdapter === "function";
    let webgpuAdapter = "not-exposed";
    if (navigatorGpu) {
      try {
        const adapter = await Promise.race([
          navigator.gpu.requestAdapter({ powerPreference: "high-performance", forceFallbackAdapter: false }),
          new Promise(resolve => setTimeout(() => resolve("timeout"), 3000)),
        ]);
        webgpuAdapter = adapter === "timeout" ? "timeout" : adapter ? "present" : "unavailable";
      } catch (error) { webgpuAdapter = "error:" + String(error); }
    }
    return { backend: "WebGL2", unmaskedRenderer, unmaskedVendor, pixel: [...p],
      glError: gl.getError(), contextLost: gl.isContextLost(), navigatorGpu, webgpuAdapter };
  });
}

export async function acquire(planFile, harness, baseline, outfile) {
  const p = validatePlan(JSON.parse(await readFile(planFile, "utf8")));
  assert.equal(process.platform, "darwin");
  assert.equal(process.arch, "arm64");
  assert.equal(process.version, "v22.23.3");
  assert.equal(process.env.RUNNER_OS, "macOS");
  assert.equal(process.env.RUNNER_ARCH, "ARM64");
  assert.equal(process.env.GITHUB_RUN_ATTEMPT, "1");
  assert.equal(process.env.GITHUB_EVENT_NAME, "pull_request");
  const event = JSON.parse(await readFile(process.env.GITHUB_EVENT_PATH, "utf8"));
  assert.equal(event.action, "opened");
  const requireFromHarness = createRequire(path.join(harness, "package.json"));
  const { chromium } = requireFromHarness("playwright");
  const { browserArgs } = await import(pathToFileURL(path.join(harness, "scripts/manim-raster-support.mjs")));
  const { productMeasurement } = await import(pathToFileURL(path.join(harness, "scripts/playground-product-fps.mjs")));
  const { serveRepository } = await import(pathToFileURL(path.join(harness, "scripts/browser-test-server.mjs")));
  const measurement = productMeasurement("parity-square-and-circle");
  let authored = await readFile(path.join(baseline, "web", measurement.sourcePath), "utf8");
  const original = "self.play(Create(circle), Create(square))";
  assert.equal(authored.split(original).length, 2, "original authored fixture changed");
  authored = authored.replace(original,
    "self.play(Create(circle), Create(square), run_time=" + measurement.windowEndSeconds + ")\n        self.wait(" + measurement.endpointHoldSeconds + ")");
  const evidence = {
    schema: 1, studyId: STUDY_ID, qualification: false, performanceAcceptance: false,
    mergeApproval: false, diagnosticOnly: true, source: p.source,
    platform: process.platform, arch: process.arch,
    node: process.version, cpu: os.cpus()[0]?.model,
    run: process.env.GITHUB_RUN_ID, attempt: process.env.GITHUB_RUN_ATTEMPT,
    cases: [], acquisitionComplete: false,
  };
  await mkdir(path.dirname(outfile), { recursive: true });
  const host = await serveRepository(baseline, 0, { crossOriginIsolated: false });
  try {
    const originalFlags = browserArgs("webgl", { gpuMode: "hardware" });
    for (const mode of FIXED_CASES) {
      const args = flagsFor(mode, originalFlags);
      const record = { id: mode.id, args, status: null, gpu: null,
        browserVersion: null, pageErrors: [], consoleErrors: [], errors: [], decision: null };
      let browser = null, context = null;
      try {
        browser = await chromium.launch({ channel: "chromium", headless: false, args,
          timeout: 30000 });
        record.browserVersion = browser.version();
        context = await browser.newContext({
          viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1,
        });
        const page = await context.newPage();
        page.on("pageerror", error => record.pageErrors.push(String(error)));
        page.on("console", msg => {
          if (msg.type() === "error") record.consoleErrors.push(msg.text());
        });
        await page.goto(host.baseUrl + "/web/index.html?example=parity-square-and-circle",
          { waitUntil: "load", timeout: 20000 });
        await page.waitForFunction(() => window.__noonExampleGallery !== undefined,
          null, { timeout: 20000 });
        record.gpu = await gpuObservation(page);
        await page.locator("#python-scene-source").evaluate((editor, text) => {
          editor.value = text;
        }, authored);
        await page.locator("#replace-scene").click({ timeout: 20000 });
        await page.waitForFunction(() => ["applied", "error"].includes(
          document.querySelector("#patch-status")?.dataset.state), null,
          { timeout: 45000 });
        record.status = await page.evaluate(() => ({
          patch: document.querySelector("#patch-status")?.dataset.state,
          backend: document.querySelector("#status")?.dataset.rendererBackend,
          status: document.querySelector("#status-text")?.textContent,
          patchMessage: document.querySelector("#patch-status")?.textContent,
        }));
      } catch (error) {
        record.errors.push(String(error?.stack ?? error));
      } finally {
        await context?.close().catch(e => record.errors.push(String(e)));
        await browser?.close().catch(e => record.errors.push(String(e)));
      }
      record.decision = classifyCase(record);
      if (record.errors.length || record.consoleErrors.length) {
        record.decision.status = "not-confirmed";
        if (record.errors.length) record.decision.blockers.push("browser/startup/playback error");
        if (record.consoleErrors.length) record.decision.blockers.push("browser console errors");
      }
      evidence.cases.push(record);
      // Preserve even partial failure and all fixed-order observations.
      await writeFile(outfile, JSON.stringify(evidence, null, 2) + "\n");
    }
    evidence.acquisitionComplete = evidence.cases.length === FIXED_CASES.length;
  } finally {
    await host.close();
    await writeFile(outfile, JSON.stringify(evidence, null, 2) + "\n");
  }
  return evidence;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [plan, harness, baseline, out] = process.argv.slice(2);
  assert.ok(plan && harness && baseline && out,
    "usage: macos-product-webgl-select.mjs PLAN HARNESS BASELINE OUTPUT");
  acquire(path.resolve(plan),path.resolve(harness),path.resolve(baseline),path.resolve(out))
    .then(data => console.log(JSON.stringify({
      studyId: STUDY_ID, complete: data.acquisitionComplete,
      cases: data.cases.map(c => ({ id:c.id, adapter:c.gpu?.webgpuAdapter,
        backend:c.status?.backend, renderer:c.gpu?.unmaskedRenderer,
        status:c.decision.status, blockers:c.decision.blockers })),
    })))
    .catch(error => { console.error(error); process.exitCode = 1; });
}