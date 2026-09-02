import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

import { coldStartMilestones, summarizeWorkers } from "../web/playground-cold-start-metrics.js";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = positiveInteger(process.env.NOON_COLD_START_PORT ?? "4182", "port");
const baseUrl = `http://127.0.0.1:${port}`;
const backend = process.env.NOON_COLD_START_BACKEND ?? "webgpu";
assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend: ${backend}`);
const examples = parseExamples(
  process.env.NOON_COLD_START_EXAMPLES ??
    "geometry:parity-create-circle,retained-text:manim-example1-text",
);
const artifactPath = path.resolve(
  repoRoot,
  process.env.NOON_COLD_START_ARTIFACT ?? `perf-artifacts/playground-cold-start-${backend}.json`,
);

const commit = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" });
const commitSha = commit.status === 0 ? commit.stdout.trim() : null;
let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

try {
  await waitForServer();
  const cases = [];
  for (const example of examples) {
    const browser = await chromium.launch({
      channel: "chromium",
      headless: true,
      args: browserArgs(backend),
    });
    try {
      const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
      const failures = [];
      const workers = [];
      const origin = monotonicNow();
      page.on("worker", (worker) => {
        workers.push({ url: worker.url(), atMs: monotonicNow() - origin });
      });
      page.on("pageerror", (error) => failures.push(`pageerror: ${error}`));
      page.on("console", (message) => {
        if (message.type() === "error") failures.push(`console: ${message.text()}`);
      });

      const navigationStart = monotonicNow();
      await page.goto(`${baseUrl}/web/?example=${encodeURIComponent(example.id)}`, {
        waitUntil: "load",
      });
      await page.waitForFunction(
        () => document.querySelector("#status")?.dataset.runtimeStartup === "deferred",
        null,
        { timeout: 60_000 },
      );
      const pageReady = monotonicNow();
      assert.equal(
        await page.evaluate(() => window.__noonExampleGallery?.selectedExampleId),
        example.id,
      );

      const runRequested = monotonicNow();
      await page.click("#replace-scene");
      await page.waitForFunction(
        () => document.querySelector("#status")?.dataset.runtimeStartup === "started-on-demand",
        null,
        { timeout: 240_000 },
      );
      const runtimeStarted = monotonicNow();
      await page.waitForFunction(
        () => {
          const draws = Number(document.querySelector("#metric-draws")?.value);
          const objects = Number(document.querySelector("#metric-objects")?.value);
          return Number.isFinite(draws) && draws > 0 && Number.isFinite(objects) && objects > 0;
        },
        null,
        { timeout: 60_000 },
      );
      const firstMetrics = monotonicNow();
      if (failures.length > 0) throw new Error(failures.join("\n"));

      const status = await page.locator("#status").evaluate((node) => ({
        ...node.dataset,
        text: node.textContent,
      }));
      const metrics = await page.evaluate(() => ({
        objects: Number(document.querySelector("#metric-objects")?.value),
        draws: Number(document.querySelector("#metric-draws")?.value),
        uploadBytes: Number(document.querySelector("#metric-upload")?.value),
      }));
      const report = {
        label: example.label,
        exampleId: example.id,
        milestones: coldStartMilestones({
          navigationStart,
          pageReady,
          runRequested,
          runtimeStarted,
          firstMetrics,
        }),
        workers: summarizeWorkers(workers),
        status,
        metrics,
      };
      cases.push(report);
      console.log(
        `${example.label}: run→runtime ${format(report.milestones.runToRuntimeMs)} ms, ` +
          `run→metrics ${format(report.milestones.runToFirstMetricsMs)} ms, ` +
          `${report.workers.total} workers (${JSON.stringify(report.workers.byRole)})`,
      );
    } finally {
      await browser.close();
    }
  }

  const artifact = {
    schemaVersion: 1,
    benchmark: "Noon public playground cold first-run topology",
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
    configuration: { backend, examples, freshBrowserProcessPerCase: true },
    note:
      "firstMetrics is the first metrics poll reporting positive object/draw counts; it is an observable proxy, not an exact GPU presentation timestamp.",
    cases,
  };
  await mkdir(path.dirname(artifactPath), { recursive: true });
  await writeFile(artifactPath, `${JSON.stringify(artifact, null, 2)}\n`, "utf8");
  console.log(`Wrote ${path.relative(repoRoot, artifactPath)}`);
} finally {
  server.kill("SIGTERM");
}

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`cold-start server did not start: ${lastError}\n${serverOutput}`);
}

function parseExamples(value) {
  const parsed = String(value)
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => {
      const separator = entry.indexOf(":");
      if (separator <= 0 || separator === entry.length - 1) {
        throw new Error(`invalid cold-start example '${entry}', expected label:id`);
      }
      return { label: entry.slice(0, separator), id: entry.slice(separator + 1) };
    });
  assert.ok(parsed.length > 0, "at least one cold-start example is required");
  return parsed;
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

function monotonicNow() {
  return performance.now();
}

function positiveInteger(value, name) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return parsed;
}

function format(value) {
  return Number(value).toFixed(2);
}
