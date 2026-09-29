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
const preloadMode = process.env.NOON_COLD_START_PRELOAD ?? "on";
assert.ok(["on", "off"].includes(preloadMode), `unknown preload mode: ${preloadMode}`);
const preloadEnabled = preloadMode === "on";
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
      if (!preloadEnabled) {
        await page.route("**/live-authoring-bootstrap.js", (route) =>
          route.fulfill({ status: 200, contentType: "text/javascript", body: "" }),
        );
      }
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
      const navigationStartEpochMs = await page.evaluate(() =>
        performance.timeOrigin + (performance.getEntriesByType("navigation")[0]?.startTime ?? 0));
      await page.waitForFunction(
        (expectedId) => window.__noonExampleGallery?.selectedExampleId === expectedId,
        example.id,
        { timeout: 60_000 },
      );
      const sourceReady = await readPerformanceMark(page, "noon-playground-source-ready");
      let firstPresented = null;
      let firstMetrics = null;
      let preloadStartedAtEpochMs = null;
      let paintGateAtEpochMs = null;
      let coldFirstEdit = null;
      if (!preloadEnabled) {
        paintGateAtEpochMs = await waitForInitialPaintGate(page);
        const deferredState = await page.evaluate(() => ({
          startup: document.querySelector("#status")?.dataset.runtimeStartup,
          runInFlight: window.__noonExampleGallery?.runInFlight,
        }));
        assert.equal(deferredState.startup, "deferred", "off arm must leave runtime deferred until the first edit");
        assert.equal(deferredState.runInFlight, false, "off arm must not submit an automatic source run");
        assert.equal(workerHandles.length, 0, "off arm must not create workers before the first edit");
        const previousGeneration = await page.evaluate(() =>
          window.__noonExampleGallery.generationDiagnostics.runGeneration);
        const editStarted = monotonicNow();
        const editStartedEpochMs = await page.evaluate(() => performance.timeOrigin + performance.now());
        await submitMeasuredSourceEdit(page, "# C6 first edit without preload");
        await waitForCompletedRun(page, previousGeneration);
        const editCompleted = monotonicNow();
        const firstSession = await waitForSessionFirstPresented(page, null);
        firstMetrics = editCompleted;
        firstPresented = summarizeSessionPresentation(
          firstSession.metrics,
          navigationStartEpochMs,
          firstSession.observedAfterEditMs,
        );
        coldFirstEdit = {
          phase: "cold-first-edit-without-preload",
          editToCompletedRunMs: editCompleted - editStarted,
          editToFirstPresentedMs: firstSession.metrics.performanceTimeOriginMs +
            firstSession.metrics.firstPresentedSessionAtMs - editStartedEpochMs,
          hostObservedAfterEditMs: firstSession.observedAfterEditMs - editStartedEpochMs,
          hostObservationLagMs: firstSession.observedAfterEditMs -
            (firstSession.metrics.performanceTimeOriginMs + firstSession.metrics.firstPresentedSessionAtMs),
          session: firstSession.metrics.presentedSession,
          milestone: "first successful renderer.render() for the first authored session; not physical scanout",
        };
      }
      if (preloadEnabled) {
        await page.waitForFunction(
          () => document.querySelector("#status")?.dataset.liveAuthoring === "ready",
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
        firstMetrics = monotonicNow();
        const firstSession = await waitForSessionFirstPresented(page, null);
        firstPresented = summarizeSessionPresentation(
          firstSession.metrics,
          navigationStartEpochMs,
          firstSession.observedAfterEditMs,
        );
        preloadStartedAtEpochMs = await readPerformanceMark(page, "noon-live-authoring-preload-start");
        paintGateAtEpochMs = preloadStartedAtEpochMs;
      }
      if (failures.length > 0) throw new Error(failures.join("\n"));
      assert.ok(paintGateAtEpochMs >= sourceReady,
        "initial source readiness must precede the two-frame preload/first-edit gate");
      if (preloadEnabled) {
        assert.ok(preloadStartedAtEpochMs >= paintGateAtEpochMs,
          "automatic preload must start only after the initial paint gate");
      }

      assert.ok(authoringWorker !== null, "the first scene run must create one Python authoring worker");
      const authoringStartup = summarizeAuthoringStartup(
        await authoringWorker.evaluate(
          () => globalThis.__noonAuthoringStartupMetrics ?? null,
        ),
        { navigationStartEpochMs },
      );
      const workerSummary = summarizeWorkers(workers);
      assert.equal(
        workerSummary.byRole.authoring,
        1,
        "cold preload must retain exactly one Python authoring worker",
      );
      const authoringWorkerEvent = workerSummary.workers.find(({ role }) => role === "authoring");
      assert.ok(authoringWorkerEvent, "first scene run must record Python worker creation");
      const preloadStarted = preloadStartedAtEpochMs === null
        ? null
        : navigationStart + preloadStartedAtEpochMs - navigationStartEpochMs;

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
      const beforeEditMetrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
      const previousPresentationSession = beforeEditMetrics?.metrics?.presentedSession;
      assert.ok(Number.isSafeInteger(previousPresentationSession),
        "warm edit baseline must identify the currently presented retained session");
      const beforeGeneration = await page.evaluate(() =>
        window.__noonExampleGallery.generationDiagnostics.runGeneration);
      const workersBeforeEdit = workerHandles.length;
      const editStarted = monotonicNow();
      const editStartedEpochMs = await page.evaluate(() => performance.timeOrigin + performance.now());
      await submitMeasuredSourceEdit(page, "# C6 warm source rerun");
      await waitForCompletedRun(page, beforeGeneration);
      const editCompleted = monotonicNow();
      const warmPresentation = await waitForSessionFirstPresented(page, previousPresentationSession);
      const warmMetrics = warmPresentation.response;
      assert.equal(workerHandles.length, workersBeforeEdit, `warm source edit must reuse durable workers: ${JSON.stringify(workers)}`);
      assert.ok(warmMetrics?.metrics?.objectCount > 0, "warm rerun must retain rendered content");
      assert.ok(warmMetrics.metrics.presentedFrames > 0, "warm rerun must present a frame");
      assert.equal(await page.locator("#scene").evaluate(node => getComputedStyle(node).visibility),
        "visible", "warm rerun must restore the canvas after its source edit");
      if (failures.length > 0) throw new Error(failures.join("\n"));
      const warmRerun = {
        editToCompletedRunMs: editCompleted - editStarted,
        firstPresentedAfterEdit: {
          measured: true,
          identity: "retained transport session from the reconciled semantic execution publication",
          previousSession: previousPresentationSession,
          presentedSession: warmPresentation.metrics.presentedSession,
          firstPresentedAtWorkerMs: warmPresentation.metrics.firstPresentedSessionAtMs,
          workerTimeOriginEpochMs: warmPresentation.metrics.performanceTimeOriginMs,
          editStartedEpochMs,
          editToFirstPresentedMs: warmPresentation.metrics.performanceTimeOriginMs +
            warmPresentation.metrics.firstPresentedSessionAtMs - editStartedEpochMs,
          hostObservedAfterEditMs: warmPresentation.observedAfterEditMs - editStartedEpochMs,
          hostObservationLagMs: warmPresentation.observedAfterEditMs -
            (warmPresentation.metrics.performanceTimeOriginMs +
             warmPresentation.metrics.firstPresentedSessionAtMs),
          milestone: "first successful renderer.render() for the new retained session; not physical scanout",
        },
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
        preloadExperiment: {
          enabled: preloadEnabled,
          sourceReadyAtEpochMs: sourceReady,
          preloadStartedAtEpochMs,
          postSourcePaintGateAtEpochMs: paintGateAtEpochMs,
          sourceReadyToPaintGateMs: paintGateAtEpochMs - sourceReady,
          sourceReadyToPreloadStartMs: preloadStartedAtEpochMs === null
            ? null
            : preloadStartedAtEpochMs - sourceReady,
          sourceReadyMeaning: "selected authored source and public gallery API are ready; automatic preload waits through two animation frames before starting",
        },
        milestones: preloadEnabled ? preloadedColdStartMilestones({
          navigationStart,
          pageReady,
          preloadStarted,
          firstMetrics,
        }) : null,
        authoringStartup,
        resourceFootprint,
        workers: workerSummary,
        warmRerun,
        firstEditComparison: preloadEnabled ? {
          phase: "first edit after automatic preload completed",
          editToCompletedRunMs: warmRerun.editToCompletedRunMs,
          editToFirstPresentedMs: warmRerun.firstPresentedAfterEdit.editToFirstPresentedMs,
          fromSession: warmRerun.firstPresentedAfterEdit.previousSession,
          toSession: warmRerun.firstPresentedAfterEdit.presentedSession,
        } : coldFirstEdit,
        status,
        metrics,
        firstPresented,
        rendererReady: firstPresented.rendererReady,
        browserEnvironment: { name: "Chromium", version: browserVersion },
      };
      caseReport = report;
      console.log(
        `${example.label} (${preloadMode}): source-ready→preload ${formatOptional(report.preloadExperiment.sourceReadyToPreloadStartMs)} ms, ` +
          `Python worker ${format(authoringStartup.totalMs)} ms ` +
          `(module graph ${format(authoringStartup.moduleGraphLoadMs)} ms, ` +
          `critical ${authoringStartup.criticalResource} ${format(authoringStartup.criticalResourceMs)} ms, ` +
          `imports ${format(authoringStartup.compatibilityImportInstallMs)} ms), ` +
          `Noon WASM ${formatBytes(resourceFootprint.noonWasm.packageBytes)} × ` +
          `${resourceFootprint.noonWasm.observedOwnerCount} observed owners = ` +
          `${formatBytes(resourceFootprint.noonWasm.packageBytesAcrossObservedOwners)} package footprint, ` +
          `${report.workers.total} workers (${JSON.stringify(report.workers.byRole)}), ` +
          `first worker-present ${format(firstPresented.navigationToFirstPresentedMs)} ms from navigation, ` +
          `renderer ready ${format(firstPresented.rendererReady.navigationToRendererReadyMs)} ms from navigation, ` +
          `first edit→completed run ${format(report.firstEditComparison.editToCompletedRunMs)} ms, ` +
          `first edit→present ${format(report.firstEditComparison.editToFirstPresentedMs)} ms`,
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
      automaticPreload: preloadEnabled,
    },
    memoryMeasurement: "Each case records 250 ms sampled aggregate RSS across the Chromium process tree. Shared pages can be counted more than once and GPU allocations outside process RSS are excluded; this is not a true instantaneous peak.",
    note:
    "firstMetrics is the first page metrics sample reporting positive object/draw counts. firstPresented is the first successful render for the exact retained transport session that reconciled the authored scene, converted to epoch with that render worker's performance.timeOrigin; a blank/prepared renderer frame is not treated as a scene presentation. This remains a renderer milestone, not physical display scanout. rendererReady records renderer/device creation after GPU setup. Session presentation, host observation and poll lag are separately recorded. Source-ready is marked once the selected source and public gallery API exist; the automatic preload-start mark is after the existing two-animation-frame paint gate. The off arm replaces only the live-authoring preload bootstrap with an empty test module and submits the same source edit after that gate. authoringStartup timestamps use the authoring worker's performance.timeOrigin and include first canonical Scene-context creation after the initial authoring run. resourceFootprint is collected from PerformanceResourceTiming on the page and every durable runtime worker after first metrics; disposable capability-probe workers remain in topology counts but are excluded because they intentionally terminate before measurement. Browser transferSize may be zero for cached or cross-origin entries; encodedBodySize/decodedBodySize are reported separately. Non-finite resource duration values are normalized to zero because duration is diagnostic-only and is not used in byte accounting. packageBytesAcrossObservedOwners multiplies the built noon_web_bg.wasm file size by workers that independently report that WASM resource; it is a package-footprint proxy, not a claim about resident WebAssembly memory. warmRerun measures the normal debounced source edit through completed authoring/reconciliation, including authored scene duration where applicable. Warm edit first-present uses the new retained transport session attached by successful semantic reconciliation and records that session’s first successful render; it does not infer a run from global frame counts or UI generation. The mobile-class profile is Chromium viewport/DPR emulation with 4x CPU throttling, not a physical iPhone measurement.",
    cases,
  };
  await mkdir(path.dirname(artifactPath), { recursive: true });
  await writeFile(artifactPath, `${JSON.stringify(artifact, null, 2)}\n`, "utf8");
  console.log(`Wrote ${path.relative(repoRoot, artifactPath)}`);
} finally {
  server.kill("SIGTERM");
}

function summarizeSessionPresentation(metrics, navigationStartEpochMs, observedAtEpochMs) {
  assert.ok(Number.isSafeInteger(metrics.presentedSession), "scene presentation must include its retained session identity");
  assert.ok(Number.isFinite(metrics.firstPresentedSessionAtMs), "scene session must have a first-present timestamp");
  assert.ok(Number.isFinite(metrics.rendererReadyAtMs), "scene session must include renderer-ready telemetry");
  const firstPresentedAtEpochMs = metrics.performanceTimeOriginMs + metrics.firstPresentedSessionAtMs;
  const rendererReadyAtEpochMs = metrics.performanceTimeOriginMs + metrics.rendererReadyAtMs;
  assert.ok(rendererReadyAtEpochMs <= firstPresentedAtEpochMs,
    "renderer-ready timestamp must precede the first scene-session presentation");
  return {
    presentedSession: metrics.presentedSession,
    rendererReady: {
      readyAtWorkerMs: metrics.rendererReadyAtMs,
      workerTimeOriginEpochMs: metrics.performanceTimeOriginMs,
      readyAtEpochMs: rendererReadyAtEpochMs,
      navigationToRendererReadyMs: rendererReadyAtEpochMs - navigationStartEpochMs,
    },
    firstPresentedAtWorkerMs: metrics.firstPresentedSessionAtMs,
    workerTimeOriginEpochMs: metrics.performanceTimeOriginMs,
    firstPresentedAtEpochMs,
    navigationStartEpochMs,
    navigationToFirstPresentedMs: firstPresentedAtEpochMs - navigationStartEpochMs,
    hostObservedAtEpochMs: observedAtEpochMs,
    observationLagMs: observedAtEpochMs - firstPresentedAtEpochMs,
    presentedFramesAtObservation: metrics.presentedFrames,
    rendererSampledAtWorkerMs: metrics.sampledAtMs,
    milestone: "first successful renderer.render() for the authored semantic session; not physical scanout",
  };
}

async function waitForSessionFirstPresented(page, previousSession) {
  const deadline = monotonicNow() + 15_000;
  while (monotonicNow() < deadline) {
    const response = await page.evaluate(() => window.__noonExampleGallery?.executionMetrics?.());
    const metrics = response?.metrics;
    if (Number.isSafeInteger(metrics?.presentedSession) &&
        metrics.presentedSession !== previousSession &&
        Number.isFinite(metrics.firstPresentedSessionAtMs) &&
        Number.isFinite(metrics.performanceTimeOriginMs)) {
      return {
        response,
        metrics,
        observedAfterEditMs: await page.evaluate(() => performance.timeOrigin + performance.now()),
      };
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`timed out waiting for a presented retained session newer than ${previousSession}`);
}

async function readPerformanceMark(page, name) {
  const timestamp = await page.evaluate((markName) => {
    const mark = performance.getEntriesByName(markName, "mark").at(-1);
    return mark ? performance.timeOrigin + mark.startTime : null;
  }, name);
  assert.ok(Number.isFinite(timestamp), `missing page performance mark '${name}'`);
  return timestamp;
}

async function waitForInitialPaintGate(page) {
  return page.evaluate(() => new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => {
      performance.mark("noon-c6-initial-paint-gate");
      resolve(performance.timeOrigin + performance.now());
    }));
  }));
}

async function submitMeasuredSourceEdit(page, note) {
  await page.evaluate((comment) => {
    const source = document.querySelector("#python-scene-source");
    source.value = `${source.value.trimEnd()}\n${comment}\n`;
    source.dispatchEvent(new Event("input", { bubbles: true }));
  }, note);
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

function formatOptional(value) {
  return value == null ? "n/a" : format(value);
}

function formatBytes(value) {
  const bytes = Number(value);
  if (!Number.isFinite(bytes) || bytes < 0) return "n/a";
  if (bytes < 1024) return `${bytes.toFixed(0)} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1024 ** 2).toFixed(2)} MiB`;
}
