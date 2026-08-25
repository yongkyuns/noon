import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(
  await readFile(path.join(repoRoot, "benchmarks/performance-scenes.json"), "utf8"),
);
assert.equal(manifest.schemaVersion, 1);
const backend = process.env.NOON_CORPUS_BACKEND ?? "webgpu";
assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend: ${backend}`);
const selected = new Set(list(process.env.NOON_CORPUS_CASES ?? ""));
const cases = manifest.cases.filter((item) => selected.size === 0 || selected.has(item.id));
assert.ok(cases.length > 0, "performance corpus selection is empty");
const warmup = positiveInteger(process.env.NOON_CORPUS_WARMUP ?? "30", "warmup");
const frames = positiveInteger(process.env.NOON_CORPUS_FRAMES ?? "180", "frames");
const targetHz = positiveNumber(process.env.NOON_CORPUS_TARGET_HZ ?? "60", "target Hz");
const enforce = process.env.NOON_CORPUS_ENFORCE_BUDGETS === "1";
const port = positiveInteger(process.env.NOON_CORPUS_PORT ?? "4178", "port");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactPath = path.resolve(
  repoRoot,
  process.env.NOON_CORPUS_ARTIFACT ?? `perf-artifacts/performance-corpus-${backend}.json`,
);

const commit = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" });
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
  browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs(backend) });
  const results = [];
  let failedBudgets = 0;
  for (const definition of cases) {
    const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
    const query = new URLSearchParams({
      source: definition.source,
      context: JSON.stringify(definition.context ?? {}),
      cameraHeight: String(definition.cameraHeight ?? 6),
      warmup: String(warmup),
      frames: String(frames),
      targetHz: String(targetHz),
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
    const budget = manifest.tiers[definition.tier]?.budgets ?? null;
    const evaluation = evaluateBudget(report, budget);
    if (!evaluation.passed) failedBudgets += 1;
    results.push({ definition, report, budget: evaluation });
    console.log(
      `${format(report.cadence.effective?.effectiveFps)} FPS, ` +
        `p95 ${format(report.cadence.frameIntervalMs?.p95)} ms, ` +
        `${evaluation.passed ? "budget ok" : "BUDGET FAIL"}`,
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
    configuration: { backend, warmup, frames, targetHz, enforce },
    results,
  };
  await mkdir(path.dirname(artifactPath), { recursive: true });
  await writeFile(artifactPath, `${JSON.stringify(artifact, null, 2)}\n`);
  console.log(`Wrote ${path.relative(repoRoot, artifactPath)}`);
  if (enforce && failedBudgets > 0) process.exitCode = 2;
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}

function evaluateBudget(report, budget) {
  if (budget === null) return { passed: true, gated: false, checks: [] };
  const actual = {
    frameIntervalP95Ms: report.cadence.frameIntervalMs?.p95,
    frameIntervalP99Ms: report.cadence.frameIntervalMs?.p99,
    longFrameRateMax: report.cadence.effective?.longFrameRate,
    cpuFrameP95Ms: report.cpu.frameMs?.p95,
    gpuP95Ms: report.gpu?.p95,
  };
  const checks = Object.entries(budget).map(([metric, limit]) => ({
    metric,
    limit,
    actual: actual[metric] ?? null,
    passed: actual[metric] == null || actual[metric] <= limit,
  }));
  return { passed: checks.every((check) => check.passed), gated: true, checks };
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

function browserArgs(mode) {
  return mode === "webgpu"
    ? ["--enable-unsafe-webgpu", "--use-gpu-in-tests", "--ignore-gpu-blocklist", "--disable-gpu-sandbox", "--disable-dev-shm-usage"]
    : ["--disable-features=WebGPU", "--ignore-gpu-blocklist", "--disable-gpu-sandbox", "--disable-dev-shm-usage"];
}

function list(value) {
  return String(value).split(",").map((item) => item.trim()).filter(Boolean);
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
