import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { evaluateBudget } from "./perf-corpus-budget.mjs";
import {
  browserArgs, classifyBrowserGpuDiagnostics, rendererGpuQualification,
} from "./manim-raster-support.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(
  await readFile(path.join(repoRoot, "benchmarks/performance-scenes.json"), "utf8"),
);
assert.equal(manifest.schemaVersion, 1);
const backend = process.env.NOON_CORPUS_BACKEND ?? "webgpu";
assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend: ${backend}`);
const gpuMode = process.env.NOON_CORPUS_GPU_MODE ?? null;
assert.ok(gpuMode === null || gpuMode === "hardware" || gpuMode === "software",
  "NOON_CORPUS_GPU_MODE must be hardware or software");
const selected = new Set(list(process.env.NOON_CORPUS_CASES ?? ""));
const cases = manifest.cases.filter((item) => selected.size === 0 || selected.has(item.id));
assert.ok(cases.length > 0, "performance corpus selection is empty");
const warmup = positiveInteger(process.env.NOON_CORPUS_WARMUP ?? "30", "warmup");
const frames = positiveInteger(process.env.NOON_CORPUS_FRAMES ?? "180", "frames");
const targetHz = positiveNumber(process.env.NOON_CORPUS_TARGET_HZ ?? "60", "target Hz");
const enforce = process.env.NOON_CORPUS_ENFORCE_BUDGETS === "1";
const includeSamples = booleanOption("NOON_CORPUS_INCLUDE_SAMPLES");
const includeRendererSamples = booleanOption("NOON_CORPUS_INCLUDE_RENDERER_SAMPLES");
const includeStageTimings = booleanOption("NOON_CORPUS_INCLUDE_STAGE_TIMINGS");
const browserMode = process.env.NOON_CORPUS_BROWSER_MODE ?? "headless";
assert.ok(browserMode === "headless" || browserMode === "headful",
  "NOON_CORPUS_BROWSER_MODE must be headless or headful");
const port = positiveInteger(process.env.NOON_CORPUS_PORT ?? "4178", "port");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactPath = path.resolve(
  repoRoot,
  process.env.NOON_CORPUS_ARTIFACT ?? `perf-artifacts/performance-corpus-${backend}.json`,
);

// Validate configuration before loading the optional browser dependency.
const { chromium } = (await import("playwright")).default;

const commit = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" });
const workingTree = await workingTreeIdentity();
let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({ channel: "chromium", headless: browserMode === "headless",
    args: gpuMode === null ? defaultBrowserArgs(backend) : browserArgs(backend, { gpuMode }) });
  const results = [];
  let failedBudgets = 0;
  let failedGpuQualifications = 0;
  for (const definition of cases) {
    const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
    const sourceFile = path.join(repoRoot, "web", definition.source.slice(2));
    const sourceSha256Before = createHash("sha256")
      .update(await readFile(sourceFile))
      .digest("hex");
    const query = new URLSearchParams({
      source: definition.source,
      context: JSON.stringify(definition.context ?? {}),
      warmup: String(warmup),
      frames: String(frames),
      targetHz: String(targetHz),
      includeSamples: includeSamples ? "1" : "0",
      includeRendererSamples: includeRendererSamples ? "1" : "0",
      rendererMetricsSampling: "sparse",
      includeStageTimings: includeStageTimings ? "1" : "0",
      includeRendererGpuIdentity: gpuMode === null ? "0" : "1",
    });
    process.stdout.write(`Corpus ${backend} ${definition.id}… `);
    await page.goto(`${baseUrl}/web/scene-perf.html?${query}`, { waitUntil: "load" });
    await page.waitForFunction(
      () => window.__NOON_SCENE_PERF__ || document.querySelector("#status")?.dataset.state === "error",
      null,
      { timeout: definition.tier === "scalability" ? 600_000 : 240_000 },
    );
    const state = await page.locator("#status").getAttribute("data-state");
    if (state === "error") {
      throw new Error(`${definition.id}: ${await page.locator("#status").textContent()}`);
    }
    const report = await page.evaluate(() => window.__NOON_SCENE_PERF__);
    assert.equal(report.environment?.rendererBackend, backend === "webgpu" ? "WebGPU" : "WebGL2",
      `${definition.id}: observed renderer backend does not match requested ${backend}`);
    const gpuQualification = gpuMode === null ? null : rendererGpuQualification(
      gpuMode, report.environment?.rendererGpuIdentity, backend,
    );
    if (gpuQualification !== null) {
      report.environment.rendererGpuIdentityClassification = gpuQualification.classification;
      report.environment.gpuQualification = gpuQualification;
      if (!gpuQualification.passed) failedGpuQualifications += 1;
    }
    const gpuDiagnostics = gpuMode !== null ? {
      api: "renderer-device",
      identityScope: "actual WGPU device backing the retained renderer",
      available: true,
      adapter: report.environment.rendererGpuIdentity,
      classification: report.environment.rendererGpuIdentityClassification,
      classificationMeaning:
        "software is a known software renderer; hardware-like-unverified is an identified non-software renderer. This does not verify physical display presentation.",
    } : await page.evaluate(async (requestedBackend) => {
      if (requestedBackend === "webgpu") {
        if (!navigator.gpu) return {
          api: "webgpu", identityScope: "separate diagnostic adapter request", available: false, adapter: null,
        };
        const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
        if (!adapter) return {
          api: "webgpu", identityScope: "separate diagnostic adapter request", available: true, adapter: null,
        };
        let info = adapter.info ?? null;
        if (!info && typeof adapter.requestAdapterInfo === "function") info = await adapter.requestAdapterInfo();
        return {
          api: "webgpu",
          identityScope: "separate diagnostic adapter request; not proof of the renderer's device",
          available: true,
          adapter: {
            vendor: String(info?.vendor ?? ""),
            architecture: String(info?.architecture ?? ""),
            device: String(info?.device ?? ""),
            description: String(info?.description ?? ""),
            isFallbackAdapter: typeof adapter.isFallbackAdapter === "boolean" ? adapter.isFallbackAdapter : null,
          },
        };
      }
      const canvas = document.createElement("canvas");
      const gl = canvas.getContext("webgl2");
      if (!gl) return {
        api: "webgl2", identityScope: "separate diagnostic canvas context", available: false, vendor: null, renderer: null,
      };
      const extension = gl.getExtension("WEBGL_debug_renderer_info");
      return {
        api: "webgl2",
        identityScope: "separate diagnostic canvas context; not proof of the renderer's device",
        available: true,
        vendor: extension ? gl.getParameter(extension.UNMASKED_VENDOR_WEBGL) : null,
        renderer: extension ? gl.getParameter(extension.UNMASKED_RENDERER_WEBGL) : null,
      };
    }, backend);
    if (gpuMode === null) {
      gpuDiagnostics.classification = classifyBrowserGpuDiagnostics(gpuDiagnostics);
      gpuDiagnostics.classificationMeaning =
        "software means a known software/fallback marker; hardware-like-unverified means an identified descriptor without such a marker; unknown means unavailable or redacted. None proves a physical device.";
    }
    const sourceSha256After = createHash("sha256")
      .update(await readFile(sourceFile))
      .digest("hex");
    assert.equal(sourceSha256After, sourceSha256Before, `${definition.id}: source changed during measurement`);
    const budget = manifest.tiers[definition.tier]?.budgets ?? null;
    const evaluation = evaluateBudget(report, budget);
    if (!evaluation.passed) failedBudgets += 1;
    results.push({
      definition,
      sourceIdentity: {
        path: definition.source,
        sha256: sourceSha256Before,
        repositoryRevision: commit.status === 0 ? commit.stdout.trim() : null,
        workingTreeClean: workingTree.clean,
      },
      browser: { name: "Chromium", version: browser.version(), mode: browserMode },
      gpuDiagnostics,
      ...(gpuQualification === null ? {} : { gpuQualification }),
      presentation: {
        scope: "browser rendering and requestAnimationFrame cadence",
        physicalDisplayPresentationVerified: false,
      },
      report,
      budget: evaluation,
    });
    console.log(
      `${format(report.cadence.effective?.effectiveFps)} FPS, ` +
        `p95 ${format(report.cadence.frameIntervalMs?.p95)} ms, ` +
        `${evaluation.passed ? "budget ok" : evaluation.complete ? "BUDGET FAIL" : "BUDGET INCOMPLETE"}`,
    );
    await page.close();
  }

  const artifact = {
    schemaVersion: 1,
    benchmark: "Noon realistic authored performance corpus",
    generatedAt: new Date().toISOString(),
    commit: commit.status === 0 ? commit.stdout.trim() : null,
    host: {
      platform: os.platform(),
      release: os.release(),
      arch: os.arch(),
      cpu: os.cpus()[0]?.model ?? null,
      logicalCpuCount: os.cpus().length,
      totalMemoryBytes: os.totalmem(),
    },
    configuration: {
      backend, warmup, frames, targetHz, enforce, browserMode,
      gpuMode,
      includeSamples, includeRendererSamples, includeStageTimings,
      rendererMetricsSampling: "sparse",
      runtimeBuildIdentityIncluded: results.every(({ report }) => report.runtimeBuild != null),
      diagnosticInstrumentationMayAffectTiming: includeRendererSamples || includeStageTimings,
    },
    workingTree,
    results,
  };
  await mkdir(path.dirname(artifactPath), { recursive: true });
  await writeFile(artifactPath, `${JSON.stringify(artifact, null, 2)}\n`);
  console.log(`Wrote ${path.relative(repoRoot, artifactPath)}`);
  if (failedGpuQualifications > 0) process.exitCode = 3;
  else if (enforce && failedBudgets > 0) process.exitCode = 2;
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/scene-perf.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Corpus server did not start: ${lastError}\n${serverOutput}`);
}

function defaultBrowserArgs(mode) {
  return mode === "webgpu"
    ? ["--enable-unsafe-webgpu", "--use-gpu-in-tests", "--ignore-gpu-blocklist", "--disable-gpu-sandbox", "--disable-dev-shm-usage"]
    : ["--disable-features=WebGPU", "--ignore-gpu-blocklist", "--disable-gpu-sandbox", "--disable-dev-shm-usage"];
}

function list(value) {
  return String(value).split(",").map((item) => item.trim()).filter(Boolean);
}

function booleanOption(name) {
  const value = process.env[name] ?? "0";
  assert.ok(value === "0" || value === "1", `${name} must be 0 or 1`);
  return value === "1";
}

async function workingTreeIdentity() {
  const status = spawnSync("git", ["status", "--porcelain", "--untracked-files=all"], {
    cwd: repoRoot, encoding: "utf8",
  });
  const diff = spawnSync("git", ["diff", "--binary", "HEAD"], { cwd: repoRoot, encoding: "buffer" });
  const entries = status.status === 0 ? status.stdout.split("\n").filter(Boolean) : null;
  const untrackedPaths = entries?.filter((entry) => entry.startsWith("?? ")).map((entry) => entry.slice(3)) ?? null;
  return {
    clean: status.status === 0 && (entries?.length ?? 1) === 0,
    statusEntries: entries,
    untrackedPaths,
    trackedDiffSha256: diff.status === 0 ? createHash("sha256").update(diff.stdout).digest("hex") : null,
  };
}

function positiveInteger(value, name) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`);
  return parsed;
}

function positiveNumber(value, name) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) throw new Error(`${name} must be positive`);
  return parsed;
}

function format(value) {
  return Number.isFinite(value) ? Number(value).toFixed(2) : "—";
}
