import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs, isIdentifiedGpuAdapter, isSoftwareGpuAdapter } from "./manim-raster-support.mjs";
import {
  STRESS_DURATION_SECONDS,
  STRESS_PERFORMANCE_CASES,
  STRESS_SAMPLE_HZ,
  STRESS_SOURCE_SHA256,
  summarizeStressCases,
  validateStressReport,
} from "./retained-dynamic-stress-perf-lib.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sourcePath = "web/python/examples/manim_parity_stress_grid.py";
const sourceSha256 = createHash("sha256").update(await readFile(path.join(root, sourcePath))).digest("hex");
assert.equal(sourceSha256, STRESS_SOURCE_SHA256, "review the complete stress workload before updating its hash");
const backend = process.env.NOON_RETAINED_STRESS_BACKEND ?? "webgpu";
assert.ok(["webgpu", "webgl"].includes(backend), "unsupported backend");
const gpuMode = process.env.NOON_RETAINED_STRESS_GPU_MODE ?? "software";
assert.ok(["software", "hardware"].includes(gpuMode), "unsupported GPU mode");
const sampleHz = integer("SAMPLE_HZ", STRESS_SAMPLE_HZ);
const loops = integer("WORKER_LOOPS", 2);
const minimumEffectiveFps = optionalPositiveNumber("MIN_EFFECTIVE_FPS");
const transports = (process.env.NOON_RETAINED_STRESS_TRANSPORTS ?? "transferable,shared").split(",");
assert.ok(transports.length > 0 && new Set(transports).size === transports.length
  && transports.every(mode => ["transferable", "shared"].includes(mode)), "invalid transports");
const artifact = path.resolve(root, process.env.NOON_RETAINED_STRESS_ARTIFACT
  ?? `perf-artifacts/shared-dynamic-stress-${backend}.json`);
const report = {
  schemaVersion: 3,
  benchmark: "Noon shared authored dynamic stress profile",
  commit: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  generatedAt: new Date().toISOString(), sourcePath, sourceSha256, backend, gpuMode, sampleHz,
  loopsPerTransport: loops,
  minimumEffectiveFps,
  measurement: "Fresh source per loop; sample round trip includes source continuation, worker, runtime and rendering",
  performanceCases: STRESS_PERFORMANCE_CASES,
  performanceBudgets: {
    status: "not-enforced",
    reason: "Cases are measured first; isolated CPU/GPU attribution and physical per-case budgets are added separately",
  },
  runs: [],
};
const server = await serveRepository(root, integer("PORT", 4192), { crossOriginIsolated: true });
let browser;
try {
  browser = await playwright.chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs(backend, { gpuMode }),
  });
  for (const transportMode of transports) {
    for (let loop = 0; loop < loops; loop += 1) {
      const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
      const errors = [];
      page.on("pageerror", error => errors.push(String(error)));
      page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
      try {
        const params = new URLSearchParams({ source: "./python/examples/manim_parity_stress_grid.py",
          warmup: "0", frames: String(STRESS_DURATION_SECONDS * sampleHz + 1),
          targetHz: String(sampleHz), includeSamples: "1", transportMode,
          sharedSlotCapacity: String(32 * 1024 * 1024) });
        await page.goto(`${server.baseUrl}/web/scene-perf.html?${params}`);
        const adapterInfo = backend === "webgpu" ? await webGpuAdapterInfo(page) : null;
        if (backend === "webgpu" && gpuMode === "hardware") {
          assert.ok(adapterInfo, "hardware WebGPU adapter unavailable");
          assert.ok(isIdentifiedGpuAdapter(adapterInfo),
            `hardware WebGPU adapter identity unavailable: ${JSON.stringify(adapterInfo)}`);
          assert.ok(!isSoftwareGpuAdapter(adapterInfo),
            `physical WebGPU run resolved to a software adapter: ${JSON.stringify(adapterInfo)}`);
        }
        await page.waitForFunction(() => ["complete", "error"].includes(document.querySelector("#status")?.dataset.state), null, { timeout: 180000 });
        const result = await page.evaluate(() => ({ state: document.querySelector("#status").dataset.state,
          status: document.querySelector("#status").value, profile: window.__NOON_SCENE_PERF__ }));
        assert.equal(result.state, "complete", result.status);
        assert.deepEqual(errors, [], "browser errors");
        const phases = validateStressReport(result.profile, {
          transportMode,
          rendererBackend: backend === "webgpu" ? "WebGPU" : "WebGL2",
          sampleHz,
          minimumEffectiveFps,
        });
        const cases = summarizeStressCases(result.profile.samples);
        report.runs.push({ transportMode, loop, adapterInfo, phases, cases, profile: result.profile });
        const effectiveFps = result.profile.cadence.effective?.effectiveFps;
        const adapter = adapterInfo ? `, adapter ${adapterSummary(adapterInfo)}` : "";
        console.log(`PASS ${backend}/${gpuMode} ${transportMode} fresh source ${loop + 1}/${loops}: ${format(effectiveFps)} FPS, ${phases.length} phases, ${cases.length} performance cases${adapter}`);
      } finally { await page.close(); }
    }
  }
  await mkdir(path.dirname(artifact), { recursive: true });
  await writeFile(artifact, JSON.stringify(report, null, 2));
  console.log(`Wrote ${artifact}`);
} finally { await browser?.close(); await server.close(); }

async function webGpuAdapterInfo(page) {
  return page.evaluate(async () => {
    if (!navigator.gpu) return null;
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
    if (!adapter) return null;
    let info = adapter.info ?? null;
    if (!info && typeof adapter.requestAdapterInfo === "function") {
      info = await adapter.requestAdapterInfo();
    }
    return {
      vendor: String(info?.vendor ?? ""),
      architecture: String(info?.architecture ?? ""),
      device: String(info?.device ?? ""),
      description: String(info?.description ?? ""),
      isFallbackAdapter: typeof adapter.isFallbackAdapter === "boolean" ? adapter.isFallbackAdapter : null,
    };
  });
}

function adapterSummary(info) {
  return [info.vendor, info.architecture, info.device, info.description]
    .map(value => String(value ?? "").trim())
    .filter(Boolean)
    .join(" / ") || "unidentified";
}

function integer(name, fallback) {
  const value = Number(process.env[`NOON_RETAINED_STRESS_${name}`] ?? fallback);
  assert.ok(Number.isSafeInteger(value) && value > 0, `${name} must be a positive integer`);
  return value;
}

function optionalPositiveNumber(name) {
  const raw = process.env[`NOON_RETAINED_STRESS_${name}`];
  if (raw === undefined || raw.trim() === "") return null;
  const value = Number(raw);
  assert.ok(Number.isFinite(value) && value > 0, `${name} must be positive and finite`);
  return value;
}

function format(value) {
  return Number.isFinite(value) ? Number(value).toFixed(2) : "—";
}
