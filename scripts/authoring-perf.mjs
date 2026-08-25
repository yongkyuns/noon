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
const port = positiveInteger(process.env.NOON_AUTHORING_PERF_PORT ?? "4177", "port");
const baseUrl = `http://127.0.0.1:${port}`;
const counts = integerList(process.env.NOON_AUTHORING_PERF_COUNTS ?? "1000,10000,100000");
const samples = positiveInteger(process.env.NOON_AUTHORING_PERF_SAMPLES ?? "3", "samples");
const scrubs = positiveInteger(process.env.NOON_AUTHORING_PERF_SCRUBS ?? "20", "scrubs");
const backend = process.env.NOON_AUTHORING_PERF_BACKEND ?? "webgpu";
assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend: ${backend}`);
const artifactPath = path.resolve(
  repoRoot,
  process.env.NOON_AUTHORING_PERF_ARTIFACT ??
    `perf-artifacts/authoring-perf-${backend}.json`,
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
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs(backend) });
  const cases = [];
  for (const objects of counts) {
    const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
    const errors = [];
    page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
    page.on("console", (message) => {
      if (message.type() === "error") errors.push(`console: ${message.text()}`);
    });
    const query = new URLSearchParams({
      objects: String(objects),
      samples: String(samples),
      scrubs: String(scrubs),
    });
    process.stdout.write(`Authoring ${backend} ${objects.toLocaleString()} objects… `);
    await page.goto(`${baseUrl}/web/authoring-perf.html?${query}`, { waitUntil: "load" });
    await page.waitForFunction(
      () => window.__NOON_AUTHORING_PERF__ || document.querySelector("#status")?.dataset.state === "error",
      null,
      { timeout: objects >= 100_000 ? 600_000 : 240_000 },
    );
    const state = await page.locator("#status").getAttribute("data-state");
    if (state === "error") {
      const message = await page.locator("#status").textContent();
      throw new Error(`${objects} objects failed: ${message}\n${errors.join("\n")}`);
    }
    const report = await page.evaluate(() => window.__NOON_AUTHORING_PERF__);
    assert.equal(report.workload.objects, objects);
    cases.push(report);
    console.log(
      `cold ${format(report.cold.timeToVisibleMs)} ms, ` +
        `unchanged p95 ${format(report.warmUnchanged.timeToVisibleMs?.p95)} ms, ` +
        `local edit p95 ${format(report.oneObjectEdit.timeToVisibleMs?.p95)} ms, ` +
        `scrub p95 ${format(report.scrub.timeToVisibleMs?.p95)} ms`,
    );
    await page.close();
  }

  const artifact = {
    schemaVersion: 1,
    benchmark: "Noon interactive authoring latency matrix",
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
    configuration: { backend, counts, samples, scrubs },
    cases,
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
      const response = await fetch(`${baseUrl}/web/authoring-perf.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Authoring performance server did not start: ${lastError}\n${serverOutput}`);
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
  const values = String(value)
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean)
    .map((item) => positiveInteger(item, "object count"));
  assert.ok(values.length > 0, "at least one object count is required");
  return values;
}

function positiveInteger(value, name) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return parsed;
}

function format(value) {
  return Number.isFinite(value) ? Number(value).toFixed(2) : "—";
}
