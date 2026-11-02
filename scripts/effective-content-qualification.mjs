// One mixed callback publication through the shared Rust path and canonical
// Python workers. These are bounded functional/pixel checks, not pacing evidence.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { startCallbackProof, finishCallbackProof, disposeCallbackProof } from "./callback-continuation-proof.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(process.env.NOON_EFFECTIVE_CONTENT_ARTIFACTS ??
  path.join(root, "browser-smoke-artifacts/effective-content"));
const times = [0, 0.0625, 0.125, 0.1875, 0.25];
const hash = value => createHash("sha256").update(value).digest("hex");
const json = value => JSON.stringify(value, null, 2) + "\n";
const sourcePath = "web/python/examples/ordinary_effective_mixed_content_updaters.py";
const asyncSource = await readFile(path.join(root, sourcePath), "utf8");
assert.equal(asyncSource.split("async def construct(self):").length, 2);
assert.equal(asyncSource.split("await self.wait(0.125)").length, 3);
const sources = {
  async: asyncSource,
  sync: asyncSource.replace("async def construct(self):", "def construct(self):")
    .replaceAll("await self.wait(0.125)", "self.wait(0.125)"),
};
const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.source.js"), "utf8"));
const report = {
  status: "running", startedAt: new Date().toISOString(),
  scope: "atomic mixed callback content; direct typed Rust/WASM and canonical Python workers; no physical-device performance or callback replay claim",
  revision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  sourceDirty: execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim() !== "",
  harnessSha256: hash(await readFile(fileURLToPath(import.meta.url))),
  helperSha256: hash(await readFile(new URL("./callback-continuation-proof.mjs", import.meta.url))),
  sources: Object.fromEntries(Object.entries(sources).map(([name, source]) =>
    [name, { bytes: Buffer.byteLength(source), sha256: hash(source) }])),
  configuration: { backends: ["webgl", "webgpu"], transports: ["transferable", "shared"],
    sourceModes: ["async", "sync"], executedTimes: times,
    renderedObservationTimes: { rustWasm: times, python: times.slice(1) },
    pythonTimeZero: "executed and asserted in source; the start helper returns after 0.0625",
    ordinaryAdmission: true },
  tolerances: { boundsSceneUnits: 1e-6, differingPixels: 0,
    colorProbeHighChannelAbove: 200, colorProbeLowChannelBelow: 40 },
  cases: [], teardownErrors: [],
};
await mkdir(output, { recursive: true });
let server, browser;
const failures = [];

function near(actual, expected, label) {
  assert.ok(Number.isFinite(actual) && Math.abs(actual - expected) < 1e-6,
    `${label}: ${actual} != ${expected}`);
}

function observe(frame, time) {
  near(frame.time, time, "displayed authored time");
  const rows = frame.objects.filter(row => row.present);
  assert.equal(rows.length, 4);
  const changed = time >= 0.125;
  const dimensions = [[0.2, 0.2], [changed ? 1.6 : 0.8, 0.6],
    [0.6, changed ? 1.2 : 0.6], [changed ? 0.8 : 0.4, changed ? 0.8 : 0.4]];
  const centers = [[0.25 + time, -1.5], [-2, 0], [0, 0], [2, 0]];
  for (const [index, row] of rows.entries()) {
    near(row.center[0], centers[index][0], `object ${index} center x`);
    near(row.center[1], centers[index][1], `object ${index} center y`);
    near(row.bounds.width, dimensions[index][0], `object ${index} width`);
    near(row.bounds.height, dimensions[index][1], `object ${index} height`);
  }
  assert.ok(rows[1].path_endpoints && rows[2].path_endpoints, "both retained paths resolve in the diagnostic resource lookup");
  return { time, identities: rows.map(row => row.id), objects: rows.map(row => ({
    center: row.center, bounds: row.bounds, endpoints: row.path_endpoints, fill: row.fill,
  })) };
}

async function bounded(operation, label, timeout = 90_000) {
  let timer;
  try {
    return await Promise.race([operation, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label} timed out`)), timeout);
    })]);
  } finally { clearTimeout(timer); }
}

async function capture(page, selector, time) {
  const bytes = await page.locator(selector).screenshot();
  const pixels = PNG.sync.read(bytes);
  assert.equal(pixels.width, 640);
  assert.equal(pixels.height, 360);
  let foreground = 0;
  for (let index = 0; index < pixels.data.length; index += 4) {
    if (Math.max(...pixels.data.subarray(index, index + 3)) > 100) foreground++;
  }
  assert.ok(foreground > 800, "mixed callback scene cannot qualify with blank pixels");
  const colors = { red: [1, 0, 0], blue: [0, 0, 1], green: [0, 1, 0],
    yellow: [1, 1, 0], black: [0, 0, 0] };
  const probes = [
    ["path_a_center", -2, 0, "red"], ["path_b_center", 0, 0, "blue"],
    ["analytic_center", 2, 0, "green"], ["witness_center", 0.25 + time, -1.5, "yellow"],
    ["path_a_growth", -1.4, 0, time < 0.125 ? "black" : "red"],
    ["path_b_growth", 0, 0.45, time < 0.125 ? "black" : "blue"],
    ["analytic_growth", 2.3, 0, time < 0.125 ? "black" : "green"],
  ].map(([label, worldX, worldY, expected]) => {
    // Same default eight-unit world viewport used by shared-authoring-smoke's
    // renderedWorldPixel helper; probes lie inside or outside known geometry.
    const pixelsPerUnit = pixels.height / 8;
    const x = Math.round(pixels.width / 2 + worldX * pixelsPerUnit);
    const y = Math.round(pixels.height / 2 - worldY * pixelsPerUnit);
    const offset = (y * pixels.width + x) * 4;
    const rgba = [...pixels.data.subarray(offset, offset + 4)];
    for (const [channel, enabled] of colors[expected].entries()) {
      assert.ok(enabled ? rgba[channel] > 200 : rgba[channel] < 40,
        `${label} at ${time}: expected ${expected}, received ${rgba}`);
    }
    return { label, world: [worldX, worldY], pixel: [x, y], expected, rgba };
  });
  return { pixels, evidence: { width: pixels.width, height: pixels.height,
    pixelSha256: hash(pixels.data), foregroundPixels: foreground, independentColorProbes: probes } };
}

async function direct(page, entry) {
  try {
    await page.evaluate(async () => {
      const wasm = await import("./pkg/noon_web.js");
      await wasm.default();
      // No host transport belongs in the typed direct Rust engine path.
      globalThis.Worker = class { constructor() { throw new Error("direct Rust callback fixture attempted worker transport"); } };
      const canvas = document.querySelector("#scene");
      canvas.width = 640; canvas.height = 360;
      const renderer = await wasm.createDirectEffectiveMixedContentSmokeRenderer(canvas.transferControlToOffscreen());
      window.mixedContentDirect = renderer;
      renderer.advanceDirectRealtime(0);
    });
    const captures = new Map();
    for (const time of times) {
      const value = await page.evaluate(async time => {
        const renderer = window.mixedContentDirect;
        const { sampleDirectProgramThrough } = await import("../scripts/direct-program-sample.mjs");
        const state = await sampleDirectProgramThrough(renderer, time, [0.125]);
        return { state, frame: JSON.parse(renderer.debugSelectionFrameJson()),
          geometryCacheMisses: renderer.lastGeometryCacheMisses() };
      }, time);
      assert.equal(value.state.backend, entry.backend === "webgl" ? "WebGL2" : "WebGPU");
      assert.ok(value.state.drawCalls > 0);
      const observation = observe(value.frame, time);
      if (entry.observations.length) assert.deepEqual(observation.identities, entry.observations[0].identities);
      if (time > 0.125) assert.equal(value.geometryCacheMisses, 0, "equal retained paths reuse warmed geometry");
      const pixels = await capture(page, "#scene", time);
      captures.set(time, pixels.pixels);
      entry.observations.push({ ...observation, geometryCacheMisses: value.geometryCacheMisses, pixels: pixels.evidence });
    }
    return captures;
  } finally {
    try { await page.evaluate(() => window.mixedContentDirect?.free()); }
    catch (error) { entry.teardownErrors.push(`direct renderer: ${error}`); }
  }
}

async function python(page, entry, reference) {
  await page.evaluate(async () => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const authoring = new PythonAuthoringClient();
    await authoring.ready();
    window.sharedAuthoringSmoke = { authoring, AuthoringExecutionClient };
  });
  const canvasId = "mixed-content-worker";
  const started = await page.evaluate(startCallbackProof, {
    source: sources[entry.mode], canvasId, pacing: "external_samples",
    sampleTime: 0.0625, transportMode: entry.transportMode,
  });
  assert.equal(started.midpoint.time, 0.0625);
  try {
    for (const time of times.slice(1)) {
      if (time === 0.25) {
        const finished = await page.evaluate(finishCallbackProof, time);
        assert.equal(finished.authored.duration, time);
        assert.equal(finished.metrics.objectCount, 4);
        entry.sourceCompleted = true;
      } else if (time !== 0.0625) {
        await page.evaluate(async time => {
          const proof = window.sharedAuthoringSmoke.callbackProof;
          await proof.wait(proof.execution.sampleToAuthoredTime(time), `mixed content sample ${time}`);
        }, time);
      }
      const value = await page.evaluate(async () => {
        const proof = window.sharedAuthoringSmoke.callbackProof;
        return { frame: await proof.execution.debugFrame(), metrics: (await proof.execution.metrics()).metrics };
      });
      assert.equal(value.metrics.backend, entry.backend === "webgl" ? "WebGL2" : "WebGPU");
      assert.ok(value.metrics.drawCalls > 0);
      const observation = observe(value.frame, time);
      if (entry.observations.length) assert.deepEqual(observation.identities, entry.observations[0].identities);
      if (time > 0.125) assert.equal(value.metrics.geometryCacheMisses, 0, "unchanged paths must reuse warmed geometry");
      const pixels = await capture(page, `#${canvasId}`, time);
      const expected = reference.get(time);
      assert.ok(expected, "typed Rust reference must precede Python comparison");
      let differingPixels = 0;
      for (let index = 0; index < expected.data.length; index += 4) {
        if (!expected.data.subarray(index, index + 4).equals(pixels.pixels.data.subarray(index, index + 4))) differingPixels++;
      }
      assert.equal(differingPixels, 0, "Rust/WASM and Python mixed callback pixels must agree on the same backend");
      entry.observations.push({ ...observation, geometryCacheMisses: value.metrics.geometryCacheMisses,
        pixels: pixels.evidence, differingPixels });
    }
  } finally {
    try { await page.evaluate(disposeCallbackProof); }
    catch (error) { entry.teardownErrors.push(`callback proof: ${error}`); }
    try { await page.evaluate(() => window.sharedAuthoringSmoke.authoring.terminate()); }
    catch (error) { entry.teardownErrors.push(`authoring worker: ${error}`); }
  }
}

try {
  for (const [key, relative] of [["wasmSha256", "web/pkg/noon_web_bg.wasm"],
    ["bindingsSha256", "web/pkg/noon_web.js"], ["workerSha256", "web/python-worker.js"]]) {
    report[key] = hash(await readFile(path.join(root, relative)));
  }
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of report.configuration.backends) {
    const executablePath = process.env.NOON_EFFECTIVE_CONTENT_CHROMIUM;
    browser = await playwright.chromium.launch({
      ...(executablePath ? { executablePath } : { channel: "chromium" }),
      headless: true, args: browserArgs(backend),
      ...(process.env.NOON_BROWSER_PROXY ? { proxy: { server: process.env.NOON_BROWSER_PROXY,
        bypass: "127.0.0.1,localhost" } } : {}),
    });
    let reference;
    for (const descriptor of [{ mode: "rust-wasm" },
      ...report.configuration.transports.flatMap(transportMode =>
        report.configuration.sourceModes.map(mode => ({ mode, transportMode })))]) {
      const entry = { id: [backend, descriptor.transportMode, descriptor.mode].filter(Boolean).join("-"),
        backend, ...descriptor, status: "running", observations: [], pageErrors: [], consoleErrors: [],
        failedResponses: [], failedRequests: [], teardownErrors: [] };
      report.cases.push(entry);
      const context = await browser.newContext({ viewport: { width: 800, height: 500 } });
      await cache.install(context);
      const page = await context.newPage();
      page.on("pageerror", error => entry.pageErrors.push(String(error)));
      page.on("console", message => { if (message.type() === "error") entry.consoleErrors.push(message.text()); });
      page.on("response", response => { if (response.status() >= 400) entry.failedResponses.push({ url: response.url(), status: response.status() }); });
      page.on("requestfailed", request => entry.failedRequests.push({ url: request.url(), error: request.failure()?.errorText }));
      try {
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });
        if (entry.mode === "rust-wasm") reference = await bounded(direct(page, entry), entry.id);
        else {
          assert.ok(reference, "direct Rust reference failed");
          await bounded(python(page, entry, reference), entry.id);
        }
        assert.deepEqual(entry.pageErrors, []);
        assert.deepEqual(entry.consoleErrors, []);
        assert.deepEqual(entry.failedResponses, []);
        assert.deepEqual(entry.failedRequests, []);
        assert.deepEqual(entry.teardownErrors, []);
        entry.status = "passed";
        console.log(`[PASS] ${entry.id}: mixed callback publication and paired pixels`);
      } catch (error) {
        entry.status = "failed";
        entry.error = error.stack ?? String(error);
        failures.push({ id: entry.id, error: entry.error });
        console.error(`[FAIL] ${entry.id}: ${error}`);
      } finally {
        try { await context.close(); }
        catch (error) { entry.teardownErrors.push(`browser context: ${error}`); }
        if (entry.teardownErrors.length && entry.status === "passed") {
          entry.status = "failed";
          entry.error = `teardown failed: ${entry.teardownErrors.join("; ")}`;
          failures.push({ id: entry.id, error: entry.error });
        }
      }
      await writeFile(path.join(output, "report.json"), json(report));
    }
    await browser.close(); browser = null;
  }
  assert.equal(report.cases.length, 10);
  assert.deepEqual(failures, [], "mixed callback qualification failed");
  report.status = "passed";
} catch (error) {
  report.status = "failed";
  report.error = error.stack ?? String(error);
  throw error;
} finally {
  try { await browser?.close(); }
  catch (error) { report.teardownErrors.push(`browser: ${error}`); }
  try { await server?.close(); }
  catch (error) { report.teardownErrors.push(`server: ${error}`); }
  if (report.teardownErrors.length) report.status = "failed";
  report.pyodideResourceCache = cache.stats();
  report.completedAt = new Date().toISOString();
  await writeFile(path.join(output, "report.json"), json(report));
  if (report.teardownErrors.length && !report.error) throw new Error(report.teardownErrors.join("; "));
}
