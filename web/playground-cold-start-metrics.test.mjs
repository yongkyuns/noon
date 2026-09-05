import assert from "node:assert/strict";
import { test } from "node:test";
import {
  classifyWorkerUrl,
  coldStartMilestones,
  preloadedColdStartMilestones,
  summarizeAuthoringStartup,
  summarizeWorkers,
  validateAuthoringStartupMetrics,
} from "./playground-cold-start-metrics.js";

test("coldStartMilestones reports phase-local durations", () => {
  assert.deepEqual(
    coldStartMilestones({
      navigationStart: 100,
      pageReady: 160,
      runRequested: 200,
      runtimeStarted: 470,
      firstMetrics: 610,
    }),
    {
      pageReadyMs: 60,
      runToRuntimeMs: 270,
      runToFirstMetricsMs: 410,
      runtimeToFirstMetricsMs: 140,
    },
  );
});

test("coldStartMilestones rejects invalid or non-monotonic timestamps", () => {
  assert.throws(
    () =>
      coldStartMilestones({
        navigationStart: 0,
        pageReady: 1,
        runRequested: 2,
        runtimeStarted: 1,
        firstMetrics: 3,
      }),
    /precedes/,
  );
  assert.throws(
    () =>
      coldStartMilestones({
        navigationStart: 0,
        pageReady: 1,
        runRequested: 2,
        runtimeStarted: 3,
        firstMetrics: Number.NaN,
      }),
    /finite non-negative/,
  );
});

test("preloadedColdStartMilestones reports the automatic preload path", () => {
  assert.deepEqual(
    preloadedColdStartMilestones({
      navigationStart: 100,
      pageReady: 170,
      preloadStarted: 205,
      firstMetrics: 760,
    }),
    {
      pageReadyMs: 70,
      pageReadyToPreloadMs: 35,
      preloadToFirstMetricsMs: 555,
      navigationToFirstMetricsMs: 660,
    },
  );
  assert.throws(
    () =>
      preloadedColdStartMilestones({
        navigationStart: 0,
        pageReady: 5,
        preloadStarted: 4,
        firstMetrics: 10,
      }),
    /precedes/,
  );
});

test("authoring startup metrics expose parallel resources and sequential bootstrap cost", () => {
  const summary = summarizeAuthoringStartup({
    version: 1,
    totalMs: 900,
    startupResourcesMs: 600,
    noonWebInitMs: 320,
    pyodideInitMs: 590,
    compatibilityBundleMs: 100,
    authoringBindingsMs: 20,
    compatibilityFsInstallMs: 30,
    compatibilityImportInstallMs: 240,
    compatibilityModuleCount: 27,
    compatibilitySourceChars: 500_000,
  });
  assert.equal(summary.criticalResource, "pyodide");
  assert.equal(summary.criticalResourceMs, 590);
  assert.equal(summary.resourceWorkMs, 1010);
  assert.equal(summary.resourceOverlapSavedMs, 410);
  assert.equal(summary.postResourceBootstrapMs, 290);
  assert.equal(summary.unattributedMs, 10);
  assert.equal(summary.compatibilityModuleCount, 27);
});

test("authoring startup metrics reject malformed diagnostic payloads", () => {
  assert.throws(
    () => validateAuthoringStartupMetrics({ version: 2 }),
    /schema version 1/,
  );
  assert.throws(
    () =>
      validateAuthoringStartupMetrics({
        version: 1,
        totalMs: Number.NaN,
        startupResourcesMs: 1,
        noonWebInitMs: 1,
        pyodideInitMs: 1,
        compatibilityBundleMs: 1,
        authoringBindingsMs: 1,
        compatibilityFsInstallMs: 1,
        compatibilityImportInstallMs: 1,
        compatibilityModuleCount: 27,
        compatibilitySourceChars: 1,
      }),
    /totalMs/,
  );
});

test("worker classification covers current authoring, engine, and render entry points", () => {
  assert.equal(classifyWorkerUrl("http://localhost/web/python-worker.js"), "authoring");
  assert.equal(classifyWorkerUrl("http://localhost/web/execution-engine-worker.js"), "engine");
  assert.equal(classifyWorkerUrl("http://localhost/web/retained-execution-engine-worker.js"), "engine");
  assert.equal(classifyWorkerUrl("http://localhost/web/execution-render-worker.js"), "render");
  assert.equal(classifyWorkerUrl("http://localhost/web/retained-execution-render-worker.js"), "render");
  assert.equal(classifyWorkerUrl("http://localhost/web/authoring-render-worker.js"), "render");
  assert.equal(classifyWorkerUrl("http://localhost/web/editor-worker.js"), "other");
});

test("summarizeWorkers preserves event order and counts worker roles", () => {
  const summary = summarizeWorkers([
    { url: "python-worker.js", atMs: 5 },
    { url: "execution-engine-worker.js", atMs: 20 },
    { url: "execution-render-worker.js", atMs: 21 },
    { url: "editor-worker.js", atMs: 30 },
  ]);
  assert.equal(summary.total, 4);
  assert.deepEqual(summary.byRole, { authoring: 1, engine: 1, render: 1, other: 1 });
  assert.deepEqual(
    summary.workers.map(({ role }) => role),
    ["authoring", "engine", "render", "other"],
  );
});
