import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { createProcessTreeRssSampler } from "./playground-cold-start-memory.mjs";

import {
  classifyWorkerUrl,
  preloadedColdStartMilestones,
  summarizeAuthoringStartup,
  summarizeResourceFootprint,
  summarizeWorkers,
} from "../web/playground-cold-start-metrics.js";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = positiveInteger(process.env.NOON_COLD_START_PORT ?? "4182", "port");
const baseUrl = `http://127.0.0.1:${port}`;
const backend = process.env.NOON_COLD_START_BACKEND ?? "webgpu";
const runtimePackageSourceRevision = process.env.NOON_COLD_START_PACKAGE_SOURCE_REVISION ?? null;
if (runtimePackageSourceRevision !== null) {
  assert.match(runtimePackageSourceRevision, /^[0-9a-f]{40}$/, "package source revision must be a full Git SHA");
}
const profile = process.env.NOON_COLD_START_PROFILE ?? "desktop";
assert.ok(["desktop", "mobile-class"].includes(profile), `unknown profile: ${profile}`);
assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend: ${backend}`);
const examples = parseExamples(
  process.env.NOON_COLD_START_EXAMPLES ??
    "geometry:parity-create-circle,retained-text:manim-example1-text",
);
const artifactPath = path.resolve(
  repoRoot,
  process.env.NOON_COLD_START_ARTIFACT ?? `perf-artifacts/playground-cold-start-${backend}.json`,
);
const noonWasmPath = path.join(repoRoot, "web", "pkg", "noon_web_bg.wasm");
const noonWasmPackageBytes = (await stat(noonWasmPath)).size;
assert.ok(noonWasmPackageBytes > 0, "built Noon WASM package must be non-empty");

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
    const browserServer = await chromium.launchServer({
      channel: "chromium",
      headless: true,
      args: browserArgs(backend),
    });
    let rssSampler = null;
    let browser = null;
    let caseReport = null;
    let browserVersion = null;
    try {
      const browserProcess = browserServer.process();
      assert.ok(browserProcess?.pid, "Chromium launch server must expose its process for RSS sampling");
      rssSampler = createProcessTreeRssSampler(browserProcess.pid);
      rssSampler.start();
      browser = await chromium.connect(browserServer.wsEndpoint());
      browserVersion = browser.version();
      const page = await browser.newPage(profile === "mobile-class"
        ? { viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, isMobile: true, hasTouch: true }
        : { viewport: { width: 1200, height: 900 } });
      if (profile === "mobile-class") {
        const cdp = await page.context().newCDPSession(page);
        await cdp.send("Emulation.setCPUThrottlingRate", { rate: 4 });
      }
      const failures = [];
      const workers = [];
      const workerHandles = [];
      const workerRoleCounts = { authoring: 0, engine: 0, render: 0, probe: 0, other: 0 };
      let authoringWorker = null;
      const origin = monotonicNow();
      page.on("worker", (worker) => {
        const event = { url: worker.url(), atMs: monotonicNow() - origin };
        const role = classifyWorkerUrl(event.url);
        const roleIndex = workerRoleCounts[role];
        workerRoleCounts[role] += 1;
        workers.push(event);
        if (role !== "probe") {
          workerHandles.push({
            worker,
            name: `${role}-${roleIndex}`,
            role,
            url: event.url,
          });
        }
        if (role === "authoring") {
          authoringWorker = worker;
        }
      });
      page.on("pageerror", (error) => failures.push(`pageerror: ${error}`));
      page.on("console", (message) => {
        if (message.type() === "error") failures.push(`console: ${message.text()}`);
      });

      const navigationStart = monotonicNow();
      await page.goto(`${baseUrl}/web/?example=${encodeURIComponent(example.id)}`, {
        waitUntil: "load",
      });
      const pageReady = monotonicNow();
      await page.waitForFunction(
        (expectedId) => window.__noonExampleGallery?.selectedExampleId === expectedId,
        example.id,
        { timeout: 60_000 },
      );
      const firstPresentedPromise = waitForFirstPresentedFrame(page).then(
        (value) => ({ value }),
        (error) => ({ error }),
      );

      await page.waitForFunction(
        () => document.querySelector("#status")?.dataset.runtimeStartup === "started-on-demand",
        null,
        { timeout: 240_000 },
      );
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
      const firstPresentedResult = await firstPresentedPromise;
      if (firstPresentedResult.error) throw firstPresentedResult.error;
      const firstPresented = firstPresentedResult.value;
      if (failures.length > 0) throw new Error(failures.join("\n"));

      assert.ok(authoringWorker !== null, "automatic preload must create the Python authoring worker");
      const authoringStartup = summarizeAuthoringStartup(
        await authoringWorker.evaluate(
          () => globalThis.__noonAuthoringStartupMetrics ?? null,
        ),
      );
      const workerSummary = summarizeWorkers(workers);
      assert.equal(
        workerSummary.byRole.authoring,
        1,
        "cold preload must retain exactly one Python authoring worker",
      );
      const authoringWorkerEvent = workerSummary.workers.find(({ role }) => role === "authoring");
      assert.ok(authoringWorkerEvent, "cold preload must record Python worker creation");
      const preloadStarted = origin + authoringWorkerEvent.atMs;

      const resourceContexts = [
        {
          name: "page",
          role: "page",
          entries: await page.evaluate(resourceTimingSnapshot),
        },
      ];
      for (const handle of workerHandles) {
        resourceContexts.push({
          name: handle.name,
          role: handle.role,
          entries: await handle.worker.evaluate(resourceTimingSnapshot),
        });
      }
      const resourceFootprint = summarizeResourceFootprint(resourceContexts, {
        noonWasmPackageBytes,
      });

      const status = await page.locator("#status").evaluate((node) => ({
        ...node.dataset,
        text: node.textContent,
      }));
      const metrics = await page.evaluate(() => ({
        objects: Number(document.querySelector("#metric-objects")?.value),
        draws: Number(document.querySelector("#metric-draws")?.value),
        uploadBytes: Number(document.querySelector("#metric-upload")?.value),
      }));
      // The source edit travels through the normal debounce/authoring/reconcile
      // path. Completion is an explicit endpoint, not a claim about first pixels.
      await waitForCompletedRun(page);
      const beforeGeneration = await page.evaluate(() =>
        window.__noonExampleGallery.generationDiagnostics.runGeneration);
      const workersBeforeEdit = workerHandles.length;
      const editStarted = monotonicNow();
      await page.evaluate(() => {
        const source = document.querySelector("#python-scene-source");
        source.value = `${source.value.trimEnd()}\n# C6 warm source rerun\n`;
        source.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await waitForCompletedRun(page, beforeGeneration);
      const editCompleted = monotonicNow();
      const warmMetrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
      assert.equal(workerHandles.length, workersBeforeEdit, `warm source edit must reuse durable workers: ${JSON.stringify(workers)}`);
      assert.ok(warmMetrics?.metrics?.objectCount > 0, "warm rerun must retain rendered content");
      assert.ok(warmMetrics.metrics.presentedFrames > 0, "warm rerun must present a frame");
      assert.equal(await page.locator("#scene").evaluate(node => getComputedStyle(node).visibility),
        "visible", "warm rerun must restore the canvas after its source edit");
      if (failures.length > 0) throw new Error(failures.join("\n"));
      const warmRerun = {
        editToCompletedRunMs: editCompleted - editStarted,
        durableWorkersBefore: workersBeforeEdit,
        durableWorkersAfter: workerHandles.length,
        metrics: {
          objectCount: warmMetrics.metrics.objectCount,
          drawCalls: warmMetrics.metrics.drawCalls,
          uploadBytes: warmMetrics.metrics.uploadBytes,
        },
      };
      const report = {
        label: example.label,
        exampleId: example.id,
        milestones: preloadedColdStartMilestones({
          navigationStart,
          pageReady,
          preloadStarted,
          firstMetrics,
        }),
        authoringStartup,
        resourceFootprint,
        workers: workerSummary,
        warmRerun,
        status,
        metrics,
        firstPresented,
        browserEnvironment: { name: "Chromium", version: browserVersion },
      };
      caseReport = report;
      console.log(
        `${example.label}: preload→metrics ${format(report.milestones.preloadToFirstMetricsMs)} ms, ` +
          `Python worker ${format(authoringStartup.totalMs)} ms ` +
          `(module graph ${format(authoringStartup.moduleGraphLoadMs)} ms, ` +
          `critical ${authoringStartup.criticalResource} ${format(authoringStartup.criticalResourceMs)} ms, ` +
          `imports ${format(authoringStartup.compatibilityImportInstallMs)} ms), ` +
          `Noon WASM ${formatBytes(resourceFootprint.noonWasm.packageBytes)} × ` +
          `${resourceFootprint.noonWasm.observedOwnerCount} observed owners = ` +
          `${formatBytes(resourceFootprint.noonWasm.packageBytesAcrossObservedOwners)} package footprint, ` +
          `${report.workers.total} workers (${JSON.stringify(report.workers.byRole)}), ` +
          `first worker-present ${format(firstPresented.navigationToFirstPresentedMs)} ms from navigation, ` +
          `warm edit→completed run ${format(warmRerun.editToCompletedRunMs)} ms`,
      );
    } finally {
      try {
          if (rssSampler !== null) {
            const memory = await rssSampler.stop();
            assert.ok(
              memory.sampleCount > 0 && memory.peakSampledRssBytes !== null,
              `Chromium RSS measurement unavailable: ${memory.samplingError ?? "no process samples"}`,
            );
            if (caseReport !== null) {
            caseReport.memory = memory;
            cases.push(caseReport);
          }
        }
      } finally {
        try {
          await browser?.close();
        } finally {
          await browserServer.close();
        }
      }
    }
  }

  const artifact = {
    schemaVersion: 5,
    benchmark: "Noon public playground preloaded cold-start topology",
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
      browser: cases[0]?.browserEnvironment ?? null,
    },
    package: {
      noonWasmPath: path.relative(repoRoot, noonWasmPath),
      noonWasmPackageBytes,
      sourceRevision: runtimePackageSourceRevision,
      frontendCommit: commitSha,
    },
    configuration: {
      backend,
      profile,
      cpuThrottlingRate: profile === "mobile-class" ? 4 : 1,
      examples,
      freshBrowserProcessPerCase: true,
      automaticPreload: true,
    },
    memoryMeasurement: "Each case records 250 ms sampled aggregate RSS across the Chromium process tree. Shared pages can be counted more than once and GPU allocations outside process RSS are excluded; this is not a true instantaneous peak.",
    note:
      "firstMetrics is the first metrics poll reporting positive object/draw counts. firstPresented records the renderer worker's first successful renderer.render() timestamp converted to epoch with that worker's performance.timeOrigin; it is a renderer-level present milestone, not physical display scanout. Its hostObservedAtPageMs and observationLagMs retain the page's later metrics observation separately. preloadStarted is the Python authoring worker creation event. authoringStartup measures that persistent worker from worker time-origin through readiness. resourceFootprint is collected from PerformanceResourceTiming on the page and every durable runtime worker after first metrics; disposable capability-probe workers remain in topology counts but are excluded because they intentionally terminate before measurement. Browser transferSize may be zero for cached or cross-origin entries; encodedBodySize/decodedBodySize are reported separately. Non-finite resource duration values are normalized to zero because duration is diagnostic-only and is not used in byte accounting. packageBytesAcrossObservedOwners multiplies the built noon_web_bg.wasm file size by workers that independently report that WASM resource; it is a package-footprint proxy, not a claim about resident WebAssembly memory. warmRerun measures the normal debounced source edit through completed authoring/reconciliation, including authored scene duration where applicable. The mobile-class profile is Chromium viewport/DPR emulation with 4x CPU throttling, not a physical iPhone measurement.",
    cases,
  };
  await mkdir(path.dirname(artifactPath), { recursive: true });
  await writeFile(artifactPath, `${JSON.stringify(artifact, null, 2)}\n`, "utf8");
  console.log(`Wrote ${path.relative(repoRoot, artifactPath)}`);
} finally {
  server.kill("SIGTERM");
}

async function waitForFirstPresentedFrame(page) {
  return page.evaluate(async () => {
    const deadline = performance.now() + 240_000;
    while (performance.now() < deadline) {
      const response = await window.__noonExampleGallery?.executionMetrics?.();
      const metrics = response?.metrics;
      if (metrics?.presentedFrames > 0 && Number.isFinite(metrics.firstPresentedAtMs) &&
          Number.isFinite(metrics.performanceTimeOriginMs)) {
        const observedAtPageMs = performance.now();
        const firstPresentedAtEpochMs = metrics.performanceTimeOriginMs + metrics.firstPresentedAtMs;
        const navigationStartEpochMs = performance.timeOrigin +
          (performance.getEntriesByType("navigation")[0]?.startTime ?? 0);
        return {
          firstPresentedAtWorkerMs: metrics.firstPresentedAtMs,
          workerTimeOriginEpochMs: metrics.performanceTimeOriginMs,
          firstPresentedAtEpochMs,
          navigationStartEpochMs,
          navigationToFirstPresentedMs: firstPresentedAtEpochMs - navigationStartEpochMs,
          hostObservedAtPageMs: observedAtPageMs,
          pageTimeOriginEpochMs: performance.timeOrigin,
          observationLagMs: performance.timeOrigin + observedAtPageMs - firstPresentedAtEpochMs,
          presentedFramesAtObservation: metrics.presentedFrames,
          rendererSampledAtWorkerMs: metrics.sampledAtMs,
        };
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error("timed out waiting for renderer first-present telemetry");
  });
}

async function waitForCompletedRun(page, previousGeneration = -1) {
  await page.waitForFunction((previous) => {
    const patch = document.querySelector("#patch-status");
    if (patch?.dataset.state === "error") return true;
    return window.__noonExampleGallery?.runInFlight === false &&
      patch?.dataset.state === "applied" && Number(patch.dataset.runGeneration) > previous;
  }, previousGeneration, { timeout: 240_000 });
  const patch = await page.locator("#patch-status").evaluate((node) => ({ state: node.dataset.state, text: node.value }));
  assert.equal(patch.state, "applied", `source run failed: ${patch.text}`);
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

function resourceTimingSnapshot() {
  return performance.getEntriesByType("resource").map((entry) => ({
    name: entry.name,
    initiatorType: entry.initiatorType,
    transferSize: entry.transferSize,
    encodedBodySize: entry.encodedBodySize,
    decodedBodySize: entry.decodedBodySize,
    duration: Number.isFinite(entry.duration) && entry.duration >= 0 ? entry.duration : 0,
  }));
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

function formatBytes(value) {
  const bytes = Number(value);
  if (!Number.isFinite(bytes) || bytes < 0) return "n/a";
  if (bytes < 1024) return `${bytes.toFixed(0)} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1024 ** 2).toFixed(2)} MiB`;
}
