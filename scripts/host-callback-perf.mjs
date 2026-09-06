import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const cases = parseCases(process.env.NOON_HOST_CALLBACK_CASES ?? "1000:1,1000:100,10000:1,10000:100");
const frames = positiveInteger(process.env.NOON_HOST_CALLBACK_FRAMES ?? "300", "frames");
const warmup = positiveInteger(process.env.NOON_HOST_CALLBACK_WARMUP ?? "30", "warmup");
const port = positiveInteger(process.env.NOON_HOST_CALLBACK_PORT ?? "4184", "port");
const artifactPath = path.resolve(repoRoot, process.env.NOON_HOST_CALLBACK_ARTIFACT ?? "perf-artifacts/host-callback-perf.json");
const baseUrl = `http://127.0.0.1:${port}`;

let serverOutput = "";
const server = spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot], {
  cwd: repoRoot,
  stdio: ["ignore", "pipe", "pipe"],
});
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: [
      "--disable-features=WebGPU",
      "--enable-unsafe-swiftshader",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ],
  });
  const results = [];
  for (const testCase of cases) {
    const page = await browser.newPage();
    const query = new URLSearchParams({
      objects: String(testCase.objects),
      active: String(testCase.active),
      frames: String(frames),
      warmup: String(warmup),
    });
    process.stdout.write(`Host callback ${testCase.active}/${testCase.objects}… `);
    await page.goto(`${baseUrl}/web/host-callback-perf.html?${query}`, { waitUntil: "load" });
    await page.waitForFunction(
      () => window.__NOON_HOST_CALLBACK_PERF__ || document.querySelector("#status")?.dataset.state === "error",
      null,
      { timeout: 600_000 },
    );
    const state = await page.locator("#status").getAttribute("data-state");
    if (state === "error") throw new Error(await page.locator("#status").textContent());
    const report = await page.evaluate(() => window.__NOON_HOST_CALLBACK_PERF__);
    assert.equal(report.schemaVersion, 2);
    assert.equal(report.workload.objects, testCase.objects);
    assert.equal(report.workload.active, testCase.active);
    assert.equal(report.native.rendererBackend, "WebGL2");
    assert.equal(report.host.rendererBackend, "WebGL2");
    assert.equal(report.native.locality.lastPublication.objectCount, testCase.objects);
    assert.equal(report.host.locality.lastPublication.objectCount, testCase.objects);
    assert.equal(report.native.finalState.playing, false);
    assert.equal(report.host.finalState.playing, false);
    results.push(report);
    console.log(
      `native ${fmt(report.native.advanceRoundTripMs?.p95)} ms, ` +
        `host ${fmt(report.host.advanceRoundTripMs?.p95)} ms, ` +
        `host upload ${fmt(report.host.locality.lastPublication.bytesUploaded)} B`,
    );
    await page.close();
  }
  const commit = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" });
  const artifact = {
    schemaVersion: 2,
    benchmark: "Noon canonical native timeline versus Python callback matrix",
    generatedAt: new Date().toISOString(),
    commit: commit.status === 0 ? commit.stdout.trim() : null,
    host: { platform: os.platform(), release: os.release(), arch: os.arch(), cpu: os.cpus()[0]?.model ?? null },
    configuration: { cases, frames, warmup, rendererBackend: "WebGL2 (SwiftShader)" },
    results,
  };
  await mkdir(path.dirname(artifactPath), { recursive: true });
  await writeFile(artifactPath, `${JSON.stringify(artifact, null, 2)}\n`);
  console.log(`Wrote ${path.relative(repoRoot, artifactPath)}`);
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/host-callback-perf.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`host callback perf server did not start: ${lastError}\n${serverOutput}`);
}

function parseCases(value) {
  return String(value).split(",").map((entry) => {
    const [objectsText, activeText] = entry.trim().split(":");
    const objects = positiveInteger(objectsText, "objects");
    const active = positiveInteger(activeText, "active");
    assert.ok(active <= objects, "active must not exceed objects");
    return { objects, active };
  });
}

function positiveInteger(value, name) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`);
  return parsed;
}

function fmt(value) {
  return Number.isFinite(value) ? Number(value).toFixed(3) : "—";
}
