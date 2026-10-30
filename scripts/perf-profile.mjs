import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { createRuntimeBuildIdentity } from "./build-runtime-identity.mjs";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_PERF_PORT ?? "4175");
const baseUrl = `http://127.0.0.1:${port}`;
const backend = process.env.NOON_PERF_BACKEND ?? "webgpu";
const forceSoftwareWebGpu = process.env.NOON_PERF_FORCE_SOFTWARE_WEBGPU === "1";
const requireGpuTimestamps = process.env.NOON_PERF_REQUIRE_GPU_TIMESTAMPS === "1";
assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend: ${backend}`);
assert.ok(
  backend === "webgpu" || !forceSoftwareWebGpu,
  "NOON_PERF_FORCE_SOFTWARE_WEBGPU requires the WebGPU backend",
);
assert.ok(
  backend === "webgpu" || !requireGpuTimestamps,
  "NOON_PERF_REQUIRE_GPU_TIMESTAMPS requires the WebGPU backend",
);

const activeGlows = process.env.NOON_PERF_GLOWS === undefined ? null
  : positiveInteger(process.env.NOON_PERF_GLOWS, "NOON_PERF_GLOWS");
const glowProperty = process.env.NOON_PERF_GLOW_PROPERTY ?? "intensity";
const counts = integerList(process.env.NOON_PERF_COUNTS ?? (activeGlows === null ? "1000,10000,100000" : "600"));
const layouts = stringList(process.env.NOON_PERF_LAYOUTS ?? (activeGlows === null ? "fit,fixed,overdraw" : "fixed"));
if (activeGlows !== null) {
  assert.ok([1, 100].includes(activeGlows), "NOON_PERF_GLOWS must be 1 or 100");
  assert.ok(["intensity", "radius"].includes(glowProperty), "NOON_PERF_GLOW_PROPERTY must be intensity or radius");
  assert.deepEqual(counts, [600], "G4 uses exactly 600 objects");
  assert.deepEqual(layouts, ["fixed"], "G4 uses the frozen fixed layout");
}
for (const layout of layouts) {
  assert.ok(["fit", "fixed", "overdraw"].includes(layout), `unknown layout: ${layout}`);
}
const warmup = positiveInteger(process.env.NOON_PERF_WARMUP ?? "30", "NOON_PERF_WARMUP");
const frames = positiveInteger(process.env.NOON_PERF_FRAMES ?? "300", "NOON_PERF_FRAMES");
const targetHz = positiveNumber(process.env.NOON_PERF_TARGET_HZ ?? "60", "NOON_PERF_TARGET_HZ");
const width = positiveInteger(process.env.NOON_PERF_WIDTH ?? (activeGlows === null ? "960" : "1152"), "NOON_PERF_WIDTH");
const height = positiveInteger(process.env.NOON_PERF_HEIGHT ?? (activeGlows === null ? "540" : "648"), "NOON_PERF_HEIGHT");
const dpr = positiveNumber(process.env.NOON_PERF_DPR ?? "1", "NOON_PERF_DPR");
const artifactPath = path.resolve(
  repoRoot,
  process.env.NOON_PERF_ARTIFACT ?? (activeGlows === null
    ? `perf-artifacts/perf-profile-${backend}.json`
    : `perf-artifacts/perf-profile-glow-${activeGlows}-${glowProperty}-${backend}.json`),
);

const commit = spawnSync("git", ["rev-parse", "HEAD"], {
  cwd: repoRoot,
  encoding: "utf8",
});
const commitSha = commit.status === 0 ? commit.stdout.trim() : null;
let runtimeBuildIdentity = null;
if (activeGlows !== null) {
  runtimeBuildIdentity = JSON.parse(await readFile(path.join(repoRoot, "web/runtime-build-identity.json"), "utf8"));
  assert.deepEqual(await createRuntimeBuildIdentity(repoRoot), runtimeBuildIdentity,
    "G4 profiling requires the exact clean source/package");
  assert.equal(runtimeBuildIdentity.sourceRevision, commitSha, "G4 source revision must be committed");
}

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
    args: browserArgs(backend, forceSoftwareWebGpu, requireGpuTimestamps),
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
        ...(activeGlows === null ? {} : { glows: String(activeGlows), glowProperty }),
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
      assert.equal(report.schemaVersion, 2);
      assert.equal(report.workload.execution, "typed-direct-rust");
      assert.equal(report.workload.objects, objects);
      assert.equal(report.workload.layout, layout);
      if (activeGlows !== null) {
        assert.equal(report.workload.family, "glow-direct");
        assert.equal(report.workload.activeGlows, activeGlows);
        assert.equal(report.workload.glowProperty, glowProperty);
        assert.equal(report.rawSamples.frames.length, frames);
        assert.ok(report.workload.driverDurationSeconds >= 10);
      }
      assert.equal(report.environment.devicePixelRatio, dpr);
      assert.equal(report.environment.rendererBackend, backend === "webgpu" ? "WebGPU" : "WebGL2",
        "observed renderer backend must match the requested profile");
      if (requireGpuTimestamps) {
        assert.equal(
          report.gpu?.timestampSupported,
          true,
          `${layout}/${objects} did not expose WebGPU timestamp queries`,
        );
        const minimumSamples = Math.min(frames, 8);
        assert.ok(
          report.gpu.samples >= minimumSamples,
          `${layout}/${objects} produced ${report.gpu.samples} GPU samples; expected at least ${minimumSamples}`,
        );
        assert.ok(
          Number.isFinite(report.gpu.renderPassMs?.p95),
          `${layout}/${objects} did not report a finite GPU render-pass p95`,
        );
        assert.equal(
          report.gpu.failed,
          0,
          `${layout}/${objects} reported ${report.gpu.failed} failed GPU timestamp samples`,
        );
        assert.ok(
          report.gpu.samples + report.gpu.dropped + report.gpu.failed >= frames,
          `${layout}/${objects} left GPU timestamp samples unaccounted for`,
        );
      }
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
    schemaVersion: 2,
    benchmark: "Noon direct Rust browser performance matrix",
    generatedAt: new Date().toISOString(),
    commit: commitSha,
    ...(runtimeBuildIdentity === null ? {} : { runtimeBuildIdentity }),
    host: {
      platform: os.platform(),
      release: os.release(),
      arch: os.arch(),
      cpu: os.cpus()[0]?.model ?? null,
      logicalCpuCount: os.cpus().length,
      totalMemoryBytes: os.totalmem(),
      node: process.version,
    },
    configuration: {
      backend,
      counts,
      layouts,
      warmup,
      frames,
      targetHz,
      width,
      height,
      dpr,
      forceSoftwareWebGpu,
      requireGpuTimestamps,
      ...(activeGlows === null ? {} : { activeGlows, glowProperty }),
    },
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

function browserArgs(mode, forceSoftware = false, requireTimestamps = false) {
  if (mode === "webgpu") {
    const timestampArgs = requireTimestamps ? ["--enable-dawn-features=allow_unsafe_apis"] : [];
    if (forceSoftware) {
      return [
        "--enable-unsafe-webgpu",
        "--enable-unsafe-swiftshader",
        "--use-webgpu-adapter=swiftshader",
        "--use-gpu-in-tests",
        "--ignore-gpu-blocklist",
        "--enable-features=Vulkan",
        "--use-gl=angle",
        "--use-angle=swiftshader",
        "--use-vulkan=swiftshader",
        "--disable-gpu-sandbox",
        "--disable-dev-shm-usage",
        ...timestampArgs,
      ];
    }
    return [
      "--enable-unsafe-webgpu",
      "--use-gpu-in-tests",
      "--ignore-gpu-blocklist",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
      ...timestampArgs,
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
