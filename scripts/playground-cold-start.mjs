import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { createProcessTreeRssSampler } from "./playground-cold-start-memory.mjs";
import { NEWEST_SOURCE, SUPERSEDED_SOURCE } from "./playground-source-edit-race-fixture.mjs";

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
const preloadEditRaceMode = process.env.NOON_COLD_START_PRELOAD_EDIT_RACE ?? "off";
assert.ok(["on", "off"].includes(preloadEditRaceMode),
  `unknown preload edit race mode: ${preloadEditRaceMode}`);
const preloadEditRaceEnabled = preloadEditRaceMode === "on";
assert.ok(!preloadEditRaceEnabled || preloadEnabled,
  "the preload edit race requires NOON_COLD_START_PRELOAD=on");
const wasmAccountingMode = process.env.NOON_COLD_START_WASM_ACCOUNTING ?? "off";
assert.ok(["on", "off"].includes(wasmAccountingMode),
  "NOON_COLD_START_WASM_ACCOUNTING must be on or off");
const wasmAccountingEnabled = wasmAccountingMode === "on";
const pythonComputeProbeMode = process.env.NOON_COLD_START_PYTHON_COMPUTE ?? "off";
assert.ok(["on", "off"].includes(pythonComputeProbeMode),
  "NOON_COLD_START_PYTHON_COMPUTE must be on or off");
const pythonComputeProbeEnabled = pythonComputeProbeMode === "on";
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
      if (wasmAccountingEnabled) await installColdStartWasmAccountingRoutes(page);
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
      const pythonComputeLoopSamplesNs = [];
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
        const text = message.text();
        const computeLoop = text.match(/^NOON_COLD_START_PYTHON_LOOP_NS:(\d+)$/);
        if (computeLoop) {
          pythonComputeLoopSamplesNs.push(Number(computeLoop[1]));
        } else if (message.type() === "error") {
          failures.push(`console: ${text}`);
        }
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
      let preloadEditRace = null;
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
        const firstRunPhases = await page.evaluate(() => window.__noonExampleGallery.runPhaseMetrics);
        assert.ok(firstRunPhases?.runGeneration > previousGeneration,
          "cold first-run phase timings must belong to the accepted first source run");
        const workerRunTiming = firstRunPhases.pythonWorkerRunTiming;
        assert.ok(Number.isFinite(workerRunTiming?.startedAtMs) &&
          Number.isFinite(workerRunTiming?.completedAtMs) &&
          Number.isFinite(workerRunTiming?.performanceTimeOriginMs) &&
          workerRunTiming.startedAtMs <= workerRunTiming.completedAtMs,
        "cold first run must report the Python worker's source execution interval");
        assert.ok(Number.isFinite(firstRunPhases.initialEngineStartStartedAtMs) &&
          Number.isFinite(firstRunPhases.initialEngineStartCompletedAtMs) &&
          firstRunPhases.initialEngineStartStartedAtMs <= firstRunPhases.initialEngineStartCompletedAtMs,
        "cold first run must report the initial semantic engine start interval");
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
          pythonWorkerExecution: {
            durationMs: workerRunTiming.completedAtMs - workerRunTiming.startedAtMs,
            startedAtWorkerMs: workerRunTiming.startedAtMs,
            completedAtWorkerMs: workerRunTiming.completedAtMs,
            workerTimeOriginEpochMs: workerRunTiming.performanceTimeOriginMs,
            meaning: "Python worker wall interval around runAuthoringSource; includes work and any awaited source continuations, excludes client/worker response handling, continuation publication and engine startup",
          },
          initialEngineStart: {
            durationMs: firstRunPhases.initialEngineStartCompletedAtMs -
              firstRunPhases.initialEngineStartStartedAtMs,
            startedAtPageMs: firstRunPhases.initialEngineStartStartedAtMs,
            completedAtPageMs: firstRunPhases.initialEngineStartCompletedAtMs,
            meaning: "page-observed initial startSemanticExecution call; includes engine attach and its initial setup, excludes prepared-renderer wait and later rendering",
          },
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
        if (preloadEditRaceEnabled) {
          preloadEditRace = await runPreloadEditRace(page);
        }
        try {
          await page.waitForFunction(
            () => document.querySelector("#status")?.dataset.liveAuthoring === "ready",
            null,
            { timeout: 240_000 },
          );
        } catch (error) {
          const startupState = await page.locator("#status").evaluate((node) => ({
            text: node.textContent,
            dataset: { ...node.dataset },
          }));
          throw new Error(`live authoring did not become ready: ${JSON.stringify({ startupState, failures })}`, { cause: error });
        }
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
      if (preloadEditRaceEnabled) {
        assert.ok(workerSummary.byRole.authoring >= 1,
          "the source race must record its authoring worker topology");
      } else {
        assert.equal(workerSummary.byRole.authoring, 1,
          "cold preload must retain exactly one Python authoring worker");
      }
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
      const unavailableResourceContexts = [];
      for (const handle of workerHandles) {
        try {
          resourceContexts.push({
            name: handle.name,
            role: handle.role,
            entries: await handle.worker.evaluate(resourceTimingSnapshot),
          });
        } catch (error) {
          if (!preloadEditRaceEnabled ||
              !String(error).includes("Target page, context or browser has been closed")) {
            throw error;
          }
          unavailableResourceContexts.push({
            name: handle.name,
            role: handle.role,
            reason: `worker was retired during the opt-in source race: ${String(error)}`,
          });
        }
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
      const noonWasmInstantiation = wasmAccountingEnabled
        ? await collectNoonWasmInstantiations(workerHandles, { preloadEditRaceEnabled })
        : disabledNoonWasmInstantiation();
      const noonWasmLinearMemory = wasmAccountingEnabled
        ? summarizeNoonWasmLinearMemory(noonWasmInstantiation)
        : disabledNoonWasmLinearMemory();
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
      const warmPhases = await page.evaluate(() => window.__noonExampleGallery.runPhaseMetrics);
      const completedRunGeneration = Number(await page.locator("#patch-status").getAttribute("data-run-generation"));
      assert.equal(warmPhases?.exampleId, example.id, "warm phase timings must belong to the selected example");
      assert.ok(completedRunGeneration > beforeGeneration,
        "warm source edit must complete a newer run generation");
      assert.equal(warmPhases?.runGeneration, completedRunGeneration,
        "warm phase timings must belong to the exact accepted run generation");
      assert.ok(Number.isFinite(warmPhases.sourceRunStartedAtMs) &&
        Number.isFinite(warmPhases.sourceRunCompletedAtMs),
      "warm source run timing boundaries must be recorded");
      assert.ok(warmPhases.sourceRunStartedAtMs <= warmPhases.sourceRunCompletedAtMs,
        "warm source run boundaries must be ordered");
      assert.ok(Array.isArray(warmPhases.reconciliations) && warmPhases.reconciliations.length > 0,
        "warm semantic reconciliation calls must be recorded");
      for (const reconciliation of warmPhases.reconciliations) {
        assert.ok(["continuation", "continuation-replay", "final"].includes(reconciliation.phase) &&
          Number.isFinite(reconciliation.startedAtMs) &&
          Number.isFinite(reconciliation.completedAtMs) &&
          reconciliation.startedAtMs <= reconciliation.completedAtMs,
        "warm semantic reconciliation boundaries must be valid");
      }
      assert.equal(typeof warmPhases.semanticContextId, "string",
        "warm phase timings must retain semantic execution identity");
      assert.equal(workerHandles.length, workersBeforeEdit, `warm source edit must reuse durable workers: ${JSON.stringify(workers)}`);
      assert.ok(warmMetrics?.metrics?.objectCount > 0, "warm rerun must retain rendered content");
      assert.ok(warmMetrics.metrics.presentedFrames > 0, "warm rerun must present a frame");
      assert.equal(await page.locator("#scene").evaluate(node => getComputedStyle(node).visibility),
        "visible", "warm rerun must restore the canvas after its source edit");
      if (failures.length > 0) throw new Error(failures.join("\n"));
      const warmRerun = {
        editToCompletedRunMs: editCompleted - editStarted,
        phases: {
          identity: {
            exampleId: warmPhases.exampleId,
            selectionGeneration: warmPhases.selectionGeneration,
            runGeneration: warmPhases.runGeneration,
            semanticContextId: warmPhases.semanticContextId,
            presentedSession: warmPresentation.metrics.presentedSession,
          },
          sourceRunRoundTripMs: warmPhases.sourceRunCompletedAtMs -
            warmPhases.sourceRunStartedAtMs,
          semanticReconcileCalls: warmPhases.reconciliations.map((call) => ({
            phase: call.phase,
            roundTripMs: call.completedAtMs - call.startedAtMs,
            overlapsSourceRun: call.startedAtMs < warmPhases.sourceRunCompletedAtMs,
          })),
          meaning: "page-observed client.run and player.reconcileSemanticExecution round trips include worker transport; continuation reconciliation can occur inside client.run, so the intervals are not additive",
        },
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
      let pythonComputeProbe = null;
      if (pythonComputeProbeEnabled) {
        const previousGeneration = await page.evaluate(() =>
          window.__noonExampleGallery.generationDiagnostics.runGeneration);
        const source = [
          "from noon import Scene, Circle",
          "from time import perf_counter_ns",
          "checksum = 0",
          "__noon_compute_loop_started_ns = perf_counter_ns()",
          "for index in range(300000):",
          "    checksum += (index * index) % 97",
          "__noon_compute_loop_elapsed_ns = perf_counter_ns() - __noon_compute_loop_started_ns",
          "print('NOON_COLD_START_PYTHON_LOOP_NS:' + str(__noon_compute_loop_elapsed_ns))",
          "class PythonComputeProbe(Scene):",
          "    def construct(self):",
          "        self.add(Circle(0.5))",
          "result = PythonComputeProbe()",
          "",
        ].join("\n");
        await page.evaluate((replacement) => {
          const editor = document.querySelector("#python-scene-source");
          editor.value = replacement;
          editor.dispatchEvent(new Event("input", { bubbles: true }));
        }, source);
        await waitForCompletedRun(page, previousGeneration);
        const phases = await page.evaluate(() => window.__noonExampleGallery.runPhaseMetrics);
        assert.ok(phases?.runGeneration > previousGeneration,
          "compute probe timings must belong to a newly accepted run");
        const worker = phases.pythonWorkerRunTiming;
        assert.ok(Number.isFinite(worker?.startedAtMs) && Number.isFinite(worker?.completedAtMs) &&
          worker.startedAtMs <= worker.completedAtMs,
        "compute probe must record Python worker execution boundaries");
        assert.ok(Array.isArray(phases.reconciliations),
          "compute probe must record semantic reconciliation separately");
        assert.equal(pythonComputeLoopSamplesNs.length, 1,
          "compute probe must report exactly one Python-timed loop sample");
        assert.ok(Number.isSafeInteger(pythonComputeLoopSamplesNs[0]) &&
          pythonComputeLoopSamplesNs[0] > 0,
        "compute probe Python loop sample must be a positive integer nanosecond duration");
        pythonComputeProbe = {
          workload: "300000-iteration Python integer arithmetic loop plus minimal one-circle scene; no play/wait continuation",
          workerCpuThrottle: "unverified; NOON_COLD_START_PROFILE throttles the Chromium page target only",
          runGeneration: phases.runGeneration,
          pythonLoopElapsedNs: pythonComputeLoopSamplesNs[0],
          pythonLoopClock: "Python time.perf_counter_ns around only the integer loop body; excludes imports, source compilation, interpreter initialization and scene construction, but is elapsed time observed inside Pyodide rather than CPU-only time",
          pythonWorkerExecutionMs: worker.completedAtMs - worker.startedAtMs,
          pythonWorkerClock: "worker performance.now around runAuthoringSource; includes imports, compilation and interpreter/setup overhead; no awaited source continuation in this fixture",
          finalReconciliationCalls: phases.reconciliations.map((call) => ({
            phase: call.phase,
            roundTripMs: call.completedAtMs - call.startedAtMs,
          })),
          meaning: "the Python-timed loop duration is a narrower elapsed-time sample inside the full worker source interval; worker source interval and page-observed reconciliation remain independent, non-additive observations",
        };
      }
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
        noonWasmInstantiation,
        noonWasmLinearMemory,
        unavailableResourceContexts,
        workers: workerSummary,
        warmRerun,
        ...(pythonComputeProbe === null ? {} : { pythonComputeProbe }),
        preloadEditRace,
        firstEditComparison: preloadEnabled ? {
          phase: preloadEditRaceEnabled
            ? "follow-up edit after the preload source race completed"
            : "first edit after automatic preload completed",
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
          `${formatBytes(resourceFootprint.noonWasm.packageBytesAcrossObservedOwners)} package footprint; ` +
          (wasmAccountingEnabled
            ? `${noonWasmInstantiation.observedInstanceCount} observed instances / ` +
              `${noonWasmInstantiation.exactInputByteLengths
                ? formatBytes(noonWasmInstantiation.observedInstantiatedBytes)
                : "n/a"} module input bytes, `
            : "WASM instance accounting off, ") +
          `${report.workers.total} workers (${JSON.stringify(report.workers.byRole)}), ` +
          `first worker-present ${format(firstPresented.navigationToFirstPresentedMs)} ms from navigation, ` +
          `renderer ready ${format(firstPresented.rendererReady.navigationToRendererReadyMs)} ms from navigation, ` +
          `first edit→completed run ${format(report.firstEditComparison.editToCompletedRunMs)} ms, ` +
          `first edit→present ${format(report.firstEditComparison.editToFirstPresentedMs)} ms` +
          (preloadEditRace === null
            ? ""
            : `, preload race newest edit→present ${format(preloadEditRace.newestEdit.editToFirstPresentedMs)} ms`),
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
    schemaVersion: 7,
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
      pageCpuThrottlingRate: profile === "mobile-class" ? 4 : 1,
      pythonWorkerCpuThrottle: "unconfigured-unverified",
      examples,
      freshBrowserProcessPerCase: true,
      automaticPreload: preloadEnabled,
      preloadEditRace: preloadEditRaceEnabled,
      noonWasmAccounting: wasmAccountingEnabled,
    },
    memoryMeasurement: "Each case records 250 ms sampled aggregate RSS across the Chromium process tree. Shared pages can be counted more than once and GPU allocations outside process RSS are excluded; this is not a true instantaneous peak.",
    wasmAccountingMeasurement: {
      enabled: wasmAccountingEnabled,
      observerEffect: wasmAccountingEnabled
        ? "Enabled runs rewrite only authoring/render worker entry responses to import the probe helper; worker network interception, helper parsing, and WebAssembly method wrappers are included in startup timings. Do not compare these startup timings directly with uninstrumented runs."
        : null,
      meaning: "When enabled, noonWasmInstantiation counts successful instances with the Noon wasm-bindgen __wbindgen_start/__wbindgen_malloc/__wbindgen_free export signature and reports exact input byte lengths for ArrayBuffer/view instantiation or identity-encoded streaming responses with Content-Length. It reports observed instance/input-byte counts, not WebAssembly.Memory capacity or physical/peak memory; missing or retired worker contexts make the result partial.",
    },
    wasmLinearMemoryMeasurement: {
      enabled: wasmAccountingEnabled,
      scope: "Each exported WebAssembly.Memory belonging to a Noon wasm-bindgen instance in the instrumented authoring/render workers; observation begins immediately after successful instantiation and ends at the completed cold source run snapshot, before the warm rerun.",
      meaning: "initialBytes and peakObservedBytes are WebAssembly.Memory.buffer.byteLength capacity, not live heap allocation, module bytes, process RSS, GPU memory, or total browser memory. Aggregate byte fields sum per-instance capacities and are not concurrent process peaks. The probe reads immediately and samples every 25 ms; WebAssembly linear memory can grow but cannot shrink, and the final snapshot rereads capacity, so it captures the high-water capacity at the run boundary even if synchronous WASM work delays interval callbacks. Missing exported memory, worker coverage, or sample errors are reported explicitly.",
      observerEffect: wasmAccountingEnabled
        ? "Same worker-entry rewriting and periodic capacity reads as WASM instance accounting; opt-in instrumentation is included in measured startup, so do not compare those timings with uninstrumented runs."
        : null,
    },
    note:
    "firstMetrics is the first page metrics sample reporting positive object/draw counts. firstPresented is the first successful render for the exact retained transport session that reconciled the authored scene, converted to epoch with that render worker's performance.timeOrigin; a blank/prepared-canvas frame is not treated as a scene presentation. This remains a renderer milestone, not physical display scanout. rendererReady records renderer/device creation after GPU setup. Session presentation, host observation and poll lag are separately recorded. Source-ready is marked once the selected source and public gallery API exist; the automatic preload-start mark is after the existing two-animation-frame paint gate. The off arm replaces only the live-authoring preload bootstrap with an empty test module and submits the same source edit after that gate. Its pythonWorkerExecution is measured in the Python worker around runAuthoringSource using the worker's own performance clock; it includes work and any awaited source continuations, excludes response handling/continuation publication and does not claim pure interpreter CPU time. initialEngineStart is measured on the page around the initial startSemanticExecution call, excluding the prepared-renderer wait and later rendering while including the call's semantic-engine attachment/setup. These clocks are reported separately and are not additive. The opt-in preload edit race dispatches two distinct full-source editor inputs in one page task while live authoring is still preloading; it reports sampled session observations only. Deterministic stale-run rejection is separately verified by playground-race-smoke. authoringStartup timestamps use the authoring worker's performance.timeOrigin and include first canonical Scene-context creation after the initial authoring run. resourceFootprint is collected from PerformanceResourceTiming on the page and every worker still evaluable after first metrics; retired workers in the opt-in race are reported separately. Disposable capability-probe workers remain in topology counts but are excluded because they intentionally terminate before measurement. Browser transferSize may be zero for cached or cross-origin entries; encodedBodySize/decodedBodySize are reported separately. Non-finite resource duration values are normalized to zero because duration is diagnostic-only and is not used in byte accounting. packageBytesAcrossObservedOwners multiplies the built noon_web_bg.wasm file size by workers that independently report that WASM resource; it is a package-footprint proxy, not a claim about resident WebAssembly memory. warmRerun measures the normal debounced source edit through completed authoring/reconciliation, including authored scene duration where applicable. Warm edit first-present uses the new retained transport session attached by successful semantic reconciliation and records that session’s first successful render; it does not infer a run from global frame counts or UI generation. The mobile-class profile is Chromium viewport/DPR emulation with 4x CPU throttling applied to the page target; dedicated authoring-worker CPU throttling is not configured or calibrated, and these results are not physical iPhone measurements.",
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

async function runPreloadEditRace(page) {
  await page.waitForFunction(() => {
    const status = document.querySelector("#status");
    return status?.dataset.liveAuthoring === "preloading" &&
      status?.dataset.authoringWarmup === "started" &&
      window.__noonExampleGallery?.runInFlight === true;
  }, null, { timeout: 240_000 });
  const preloadStartedAtEpochMs = await readPerformanceMark(page, "noon-live-authoring-preload-start");
  const beforeMetrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
  const baselineSession = beforeMetrics?.metrics?.presentedSession ?? null;
  const startingGeneration = await page.evaluate(() =>
    window.__noonExampleGallery.generationDiagnostics.runGeneration);
  const editReceipts = await page.evaluate(({ superseded, newest }) => {
    const editor = document.querySelector("#python-scene-source");
    const gallery = window.__noonExampleGallery;
    const status = document.querySelector("#status");
    const submit = (source) => {
      editor.value = source;
      editor.dispatchEvent(new Event("input", { bubbles: true }));
      return {
        submittedAtEpochMs: performance.timeOrigin + performance.now(),
        generation: gallery.generationDiagnostics.runGeneration,
        liveAuthoring: status.dataset.liveAuthoring,
        authoringWarmup: status.dataset.authoringWarmup,
      };
    };
    const first = submit(superseded);
    // Dispatch the newer full-source edit in the same page task: the ordinary
    // 500 ms source debounce cannot start the obsolete source between edits.
    const latest = submit(newest);
    return {
      first,
      latest,
      runInFlightAfterBoth: gallery.runInFlight,
      source: editor.value,
    };
  }, { superseded: SUPERSEDED_SOURCE, newest: NEWEST_SOURCE });
  assert.equal(editReceipts.first.liveAuthoring, "preloading",
    "first source edit must land during automatic preload");
  assert.equal(editReceipts.latest.liveAuthoring, "preloading",
    "newest source edit must also land before the automatic preload reports ready");
  assert.ok(editReceipts.first.generation > startingGeneration &&
    editReceipts.latest.generation >= editReceipts.first.generation,
  "source edits must invalidate the active run and retain the newest editor value");
  assert.equal(editReceipts.source, NEWEST_SOURCE);
  assert.equal(editReceipts.runInFlightAfterBoth, true);
  const newestEditStartedEpochMs = editReceipts.latest.submittedAtEpochMs;
  const newestInputGeneration = editReceipts.latest.generation;

  const sampledSessions = new Map();
  const obsoleteSourceSamples = [];
  const deadline = monotonicNow() + 240_000;
  let newestRunGeneration = null;
  let newestSession = null;
  let newestSessionMetrics = null;
  let newestSessionObservedAtEpochMs = null;
  while (monotonicNow() < deadline) {
    const snapshot = await page.evaluate(async () => ({
      diagnostics: window.__noonExampleGallery.generationDiagnostics,
      runInFlight: window.__noonExampleGallery.runInFlight,
      status: {
        state: document.querySelector("#patch-status")?.dataset.state,
        runGeneration: Number(document.querySelector("#patch-status")?.dataset.runGeneration),
        text: document.querySelector("#patch-status")?.value,
      },
      source: document.querySelector("#python-scene-source")?.value,
      metrics: await window.__noonExampleGallery.executionMetrics(),
      observedAtEpochMs: performance.timeOrigin + performance.now(),
    }));
    assert.equal(snapshot.source, NEWEST_SOURCE, "newest source text must remain selected");
    if (snapshot.status.state === "error") {
      throw new Error(`preload edit race failed: ${snapshot.status.text}`);
    }
    const metrics = snapshot.metrics?.metrics;
    if (Number.isSafeInteger(metrics?.presentedSession)) {
      const session = metrics.presentedSession;
      const prior = sampledSessions.get(session);
      sampledSessions.set(session, {
        session,
        objectCount: metrics.objectCount,
        firstPresentedSessionAtMs: metrics.firstPresentedSessionAtMs,
        performanceTimeOriginMs: metrics.performanceTimeOriginMs,
        firstObservedAtEpochMs: prior?.firstObservedAtEpochMs ?? snapshot.observedAtEpochMs,
      });
      if (session !== baselineSession && metrics.objectCount === 1) {
        obsoleteSourceSamples.push(session);
      }
    }
    if (snapshot.status.state === "applied" &&
        Number.isSafeInteger(snapshot.status.runGeneration) &&
        snapshot.status.runGeneration > newestInputGeneration) {
      newestRunGeneration = snapshot.status.runGeneration;
      assert.equal(snapshot.diagnostics.runGeneration, newestRunGeneration,
        "latest accepted source must own the current run generation");
      if (Number.isSafeInteger(metrics?.presentedSession) &&
          metrics.presentedSession !== baselineSession &&
          metrics.objectCount === 2 &&
          Number.isFinite(metrics.firstPresentedSessionAtMs)) {
        newestSession = metrics.presentedSession;
        newestSessionMetrics = metrics;
        newestSessionObservedAtEpochMs = snapshot.observedAtEpochMs;
        break;
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  assert.ok(Number.isSafeInteger(newestRunGeneration), "newest edited source run must complete");
  assert.ok(Number.isSafeInteger(newestSession), "newest two-object source session must present");
  assert.deepEqual(obsoleteSourceSamples, [],
    "no sampled post-newest-edit retained session may show the one-object superseded scene");
  const finalWarmupState = await page.evaluate(() => ({
    liveAuthoring: document.querySelector("#status")?.dataset.liveAuthoring,
    authoringWarmup: document.querySelector("#status")?.dataset.authoringWarmup,
  }));
  assert.equal(finalWarmupState.liveAuthoring, "ready");
  assert.equal(newestSessionMetrics.objectCount, 2,
    "newest source's distinct two-object content must own its rendered session");
  const firstPresentedAtEpochMs = newestSessionMetrics.performanceTimeOriginMs +
    newestSessionMetrics.firstPresentedSessionAtMs;
  return {
    measured: true,
    preloadStartedAtEpochMs,
    startingGeneration,
    firstEdit: {
      sha256: sha256(SUPERSEDED_SOURCE),
      submittedAtEpochMs: editReceipts.first.submittedAtEpochMs,
      invalidationGeneration: editReceipts.first.generation,
      oneObjectScene: true,
      supersededBeforeDebouncedRun: true,
    },
    newestEdit: {
      sha256: sha256(NEWEST_SOURCE),
      submittedAtEpochMs: newestEditStartedEpochMs,
      runGeneration: newestRunGeneration,
      renderedSession: newestSession,
      renderedObjectCount: newestSessionMetrics.objectCount,
      firstPresentedAtWorkerMs: newestSessionMetrics.firstPresentedSessionAtMs,
      workerTimeOriginEpochMs: newestSessionMetrics.performanceTimeOriginMs,
      firstPresentedAtEpochMs,
      editToFirstPresentedMs: firstPresentedAtEpochMs - newestEditStartedEpochMs,
      hostObservedAtEpochMs: newestSessionObservedAtEpochMs,
    },
    bothEditsBeforeWarmupReady: editReceipts.first.liveAuthoring === "preloading" &&
      editReceipts.latest.liveAuthoring === "preloading",
    controllerGenerations: {
      beforeEdits: startingGeneration,
      afterSupersededEdit: editReceipts.first.generation,
      afterNewestEdit: editReceipts.latest.generation,
      newestAcceptedRun: newestRunGeneration,
    },
    sampledSessionObservations: [...sampledSessions.values()],
    sampledObsoleteSessionsAfterNewestEdit: obsoleteSourceSamples,
    obsoletePublicationProof: "sampled metrics only; deterministic stale-run rejection is covered by playground-race-smoke",
    warmupAfterNewestRun: finalWarmupState,
  };
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

async function installColdStartWasmAccountingRoutes(page) {
  const workerEntrypoints = new Set(["/web/python-worker.js", "/web/execution-render-worker.js"]);
  await page.route((url) => workerEntrypoints.has(new URL(url).pathname), async (route) => {
    // The authoring worker fetches and verifies its own original source against
    // runtime-build-identity.json. Instrument only the Worker script request;
    // leave that later verification fetch byte-for-byte untouched.
    if (!["script", "worker"].includes(route.request().resourceType())) {
      await route.continue();
      return;
    }
    const response = await route.fetch();
    const originalSource = await response.text();
    const injectedSource = `import "./playground-cold-start-wasm-accounting.js";\n${originalSource}`;
    const headers = { ...response.headers() };
    delete headers["content-length"];
    delete headers["content-encoding"];
    await route.fulfill({ response, body: injectedSource, headers });
  });
}

async function collectNoonWasmInstantiations(workerHandles, { preloadEditRaceEnabled }) {
  const contexts = [];
  const unavailable = [];
  for (const handle of workerHandles.filter(({ role }) => role === "authoring" || role === "render")) {
    try {
      const accounting = await handle.worker.evaluate(() => {
        const report = globalThis.__noonColdStartWasmAccounting;
        if (!report) return null;
        return {
          schemaVersion: report.schemaVersion,
          role: report.role,
          records: [...report.records],
          linearMemory: report.snapshot(),
        };
      });
      contexts.push({
        worker: handle.name,
        role: handle.role,
        instrumentationInstalled: accounting !== null,
        report: accounting,
      });
    } catch (error) {
      if (!preloadEditRaceEnabled ||
          !String(error).includes("Target page, context or browser has been closed")) {
        throw error;
      }
      unavailable.push({
        worker: handle.name,
        role: handle.role,
        reason: `worker was retired before the instance snapshot: ${String(error)}`,
      });
    }
  }

  const allRecords = contexts.flatMap((context) =>
    (context.report?.records ?? []).map((record) => ({
      worker: context.worker,
      role: context.role,
      ...record,
    })));
  const allWorkersInstrumented = unavailable.length === 0 &&
    ["authoring", "render"].every((role) => contexts.some((context) => context.role === role)) &&
    contexts.every(({ instrumentationInstalled }) => instrumentationInstalled);
  const everyByteLengthExact = allRecords.every(({ instantiatedBytes }) => Number.isSafeInteger(instantiatedBytes));
  return {
    enabled: true,
    measurement: "successful WebAssembly instances identified by Noon wasm-bindgen exports in probe-instrumented authoring/render workers",
    observedInstanceCount: allRecords.length,
    observedInstantiatedBytes: allRecords.length > 0 && everyByteLengthExact
      ? allRecords.reduce((total, { instantiatedBytes }) => total + instantiatedBytes, 0)
      : null,
    exactInputByteLengths: allRecords.length > 0 && everyByteLengthExact,
    completeWorkerCoverage: allWorkersInstrumented,
    everyInstrumentedWorkerProducedNoonInstance: contexts.length > 0 &&
      contexts.every(({ report }) => (report?.records.length ?? 0) > 0),
    contexts,
    unavailable,
    records: allRecords,
  };
}

function disabledNoonWasmInstantiation() {
  return {
    enabled: false,
    observedInstanceCount: null,
    observedInstantiatedBytes: null,
    exactInputByteLengths: false,
    completeWorkerCoverage: false,
    everyInstrumentedWorkerProducedNoonInstance: false,
    contexts: [],
    unavailable: [],
    records: [],
  };
}

function summarizeNoonWasmLinearMemory(instantiation) {
  const instances = instantiation.contexts.flatMap((context) => {
    const records = context.report?.records ?? [];
    const memory = context.report?.linearMemory ?? [];
    return records.map((record) => ({
      worker: context.worker,
      role: context.role,
      instanceOrdinal: record.ordinal,
      ...(memory.find(({ ordinal }) => ordinal === record.ordinal) ?? {
        available: false,
        reason: "linear-memory snapshot missing for this Noon instance",
      }),
    }));
  });
  const measured = instances.filter(({ available }) => available);
  const complete = instantiation.completeWorkerCoverage &&
    instantiation.everyInstrumentedWorkerProducedNoonInstance &&
    ["authoring", "render"].every((role) =>
      instances.some((instance) => instance.role === role && instance.available)) &&
    instances.length > 0 &&
    measured.length === instances.length &&
    measured.every(({ samplingError, initialBytes, peakObservedBytes, latestBytes, sampleCount }) =>
      samplingError === null && Number.isSafeInteger(initialBytes) &&
      Number.isSafeInteger(peakObservedBytes) && peakObservedBytes >= initialBytes &&
      Number.isSafeInteger(latestBytes) && sampleCount > 0);
  const byRole = Object.fromEntries(["authoring", "render"].map((role) => {
    const roleInstances = measured.filter((instance) => instance.role === role);
    return [role, {
      instanceCount: roleInstances.length,
      sumOfInstanceInitialBytes: complete
        ? roleInstances.reduce((total, { initialBytes }) => total + initialBytes, 0)
        : null,
      sumOfInstancePeakBytes: complete
        ? roleInstances.reduce((total, { peakObservedBytes }) => total + peakObservedBytes, 0)
        : null,
    }];
  }));
  return {
    enabled: true,
    measurement: "WebAssembly.Memory capacity at the completed cold source-run boundary, for each observed Noon wasm-bindgen instance",
    workerCoverage: instantiation.completeWorkerCoverage,
    complete,
    workerCount: new Set(measured.map(({ worker }) => worker)).size,
    instanceCount: instances.length,
    sumOfInstanceInitialBytes: complete
      ? measured.reduce((total, { initialBytes }) => total + initialBytes, 0)
      : null,
    sumOfInstancePeakBytes: complete
      ? measured.reduce((total, { peakObservedBytes }) => total + peakObservedBytes, 0)
      : null,
    byRole,
    instances,
  };
}

function disabledNoonWasmLinearMemory() {
  return {
    enabled: false,
    complete: false,
    workerCoverage: false,
    workerCount: null,
    instanceCount: null,
    sumOfInstanceInitialBytes: null,
    sumOfInstancePeakBytes: null,
    byRole: null,
    instances: [],
  };
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
