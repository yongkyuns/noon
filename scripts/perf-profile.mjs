import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_PERF_PORT ?? "4175");
const baseUrl = `http://127.0.0.1:${port}`;
const backend = process.env.NOON_PERF_BACKEND ?? "webgpu";
assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend: ${backend}`);

const counts = integerList(process.env.NOON_PERF_COUNTS ?? "1000,10000,100000");
const layouts = stringList(process.env.NOON_PERF_LAYOUTS ?? "fit,fixed,overdraw");
for (const layout of layouts) {
  assert.ok(["fit", "fixed", "overdraw"].includes(layout), `unknown layout: ${layout}`);
}
const warmup = positiveInteger(process.env.NOON_PERF_WARMUP ?? "30", "NOON_PERF_WARMUP");
const frames = positiveInteger(process.env.NOON_PERF_FRAMES ?? "300", "NOON_PERF_FRAMES");
const targetHz = positiveNumber(process.env.NOON_PERF_TARGET_HZ ?? "60", "NOON_PERF_TARGET_HZ");
const width = positiveInteger(process.env.NOON_PERF_WIDTH ?? "960", "NOON_PERF_WIDTH");
const height = positiveInteger(process.env.NOON_PERF_HEIGHT ?? "540", "NOON_PERF_HEIGHT");
const dpr = positiveNumber(process.env.NOON_PERF_DPR ?? "1", "NOON_PERF_DPR");
const artifactPath = path.resolve(
  repoRoot,
  process.env.NOON_PERF_ARTIFACT ?? `perf-artifacts/perf-profile-${backend}.json`,
);

const commit = spawnSync("git", ["rev-parse", "HEAD"], {
  cwd: repoRoot,
  encoding: "utf8",
});
const commitSha = commit.status === 0 ? commit.stdout.trim() : null;

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => {
  serverOutput += chunk;
});
server.stderr.on("data", (chunk) => {
  serverOutput += chunk;
});

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs(backend),
  });

  const results = [];
  for (const layout of layouts) {
    for (const objects of counts) {
      const context = await browser.newContext({
        viewport: { width: 1200, height: 900 },
        deviceScaleFactor: dpr,
      });
      const page = await context.newPage();
      const errors = [];
      page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
      page.on("console", (message) => {
        if (message.type() === "error") {
          errors.push(`console: ${message.text()}`);
        }
      });

      const query = new URLSearchParams({
        objects: String(objects),
        layout,
        warmup: String(warmup),
        frames: String(frames),
        targetHz: String(targetHz),
        width: String(width),
        height: String(height),
      });
      process.stdout.write(`Profiling ${backend} ${layout} ${objects.toLocaleString()} objects… `);
      await page.goto(`${baseUrl}/web/perf-profile.html?${query}`, { waitUntil: "load" });
      await page.waitForFunction(
        () => window.__NOON_PERF_REPORT__ || document.querySelector("#status")?.dataset.state === "error",
        null,
        { timeout: 180_000 },
      );
      const state = await page.locator("#status").getAttribute("data-state");
      if (state === "error") {
        const message = await page.locator("#status").textContent();
        throw new Error(`${layout}/${objects} failed: ${message}\n${errors.join("\n")}`);
      }
      const report = await page.evaluate(() => window.__NOON_PERF_REPORT__);
      assert.equal(report.workload.objects, objects);
      assert.equal(report.workload.layout, layout);
      assert.equal(report.environment.devicePixelRatio, dpr);
      results.push(report);
      console.log(
        `${format(report.cadence.effective?.effectiveFps)} FPS, ` +
          `p95 ${format(report.cadence.frameIntervalMs?.p95)} ms, ` +
          `CPU ${format(report.cpu.frameMs?.p95)} ms, ` +
          `GPU ${format(report.gpu.renderPassMs?.p95)} ms`,
      );
      await context.close();
    }
  }

  const artifact = {
    schemaVersion: 1,
    benchmark: "Noon canonical browser performance matrix",
    generatedAt: new Date().toISOString(),
    commit: commitSha,
    host: {
      platform: os.platform(),
      release: os.release(),
      arch: os.arch(),
      cpu: os.cpus()[0]?.model ?? null,
      logicalCpuCount: os.cpus().length,
      totalMemoryBytes: os.totalmem(),
      node: process.version,
    },
    configuration: { backend, counts, layouts, warmup, frames, targetHz, width, height, dpr },
    cases: results,
  };
  await mkdir(path.dirname(artifactPath), { recursive: true });
  await writeFile(artifactPath, `${JSON.stringify(artifact, null, 2)}\n`, "utf8");
  console.log(`Wrote ${path.relative(repoRoot, artifactPath)}`);
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/perf-profile.html`);
      if (response.ok) {
        return;
      }
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Performance server did not start: ${lastError}\n${serverOutput}`);
}

function browserArgs(mode) {
  if (mode === "webgpu") {
    return [
      "--enable-unsafe-webgpu",
      "--use-gpu-in-tests",
      "--ignore-gpu-blocklist",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ];
  }
  return [
    "--disable-features=WebGPU",
    "--ignore-gpu-blocklist",
    "--disable-gpu-sandbox",
    "--disable-dev-shm-usage",
  ];
}

function integerList(value) {
  const values = stringList(value).map((item) => positiveInteger(item, "count"));
  assert.ok(values.length > 0, "at least one object count is required");
  return values;
}

function stringList(value) {
  return String(value)
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function positiveInteger(value, name) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must contain positive integers`);
  }
  return parsed;
}

function positiveNumber(value, name) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${name} must be positive`);
  }
  return parsed;
}

function format(value) {
  return Number.isFinite(value) ? Number(value).toFixed(2) : "—";
}
