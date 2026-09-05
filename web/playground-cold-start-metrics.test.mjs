import assert from "node:assert/strict";
import { test } from "node:test";
import {
  classifyColdStartResource,
  classifyWorkerUrl,
  coldStartMilestones,
  preloadedColdStartMilestones,
  summarizeAuthoringStartup,
  summarizeResourceFootprint,
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

test("authoring startup metrics expose module graph, parallel resources, and sequential bootstrap cost", () => {
  const summary = summarizeAuthoringStartup({
    version: 1,
    totalMs: 1050,
    moduleGraphLoadMs: 150,
    initializeMs: 900,
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
  assert.equal(summary.moduleGraphLoadMs, 150);
  assert.equal(summary.initializeMs, 900);
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
        moduleGraphLoadMs: 1,
        initializeMs: 1,
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

test("cold-start resource classification distinguishes Noon and Pyodide WASM", () => {
  assert.equal(classifyColdStartResource("http://localhost/web/pkg/noon_web_bg.wasm"), "noon-wasm");
  assert.equal(
    classifyColdStartResource("https://cdn.jsdelivr.net/pyodide/v314.0.5/full/pyodide.asm.wasm"),
    "pyodide-wasm",
  );
  assert.equal(
    classifyColdStartResource("https://cdn.jsdelivr.net/pyodide/v314.0.5/full/pyodide.mjs"),
    "pyodide",
  );
  assert.equal(classifyColdStartResource("http://localhost/runtime.wasm?x=1"), "wasm");
  assert.equal(
    classifyColdStartResource("http://localhost/web/python/compat-bundle.abc123.json"),
    "compat-bundle",
  );
});

test("resource footprint preserves per-context transfer sizes and counts repeated Noon WASM owners", () => {
  const entry = (name, transferSize, encodedBodySize, decodedBodySize, duration = 10) => ({
    name,
    initiatorType: "fetch",
    transferSize,
    encodedBodySize,
    decodedBodySize,
    duration,
  });
  const summary = summarizeResourceFootprint(
    [
      {
        name: "page",
        role: "page",
        entries: [entry("http://localhost/web/main.js", 1200, 900, 900)],
      },
      {
        name: "authoring-0",
        role: "authoring",
        entries: [
          entry("http://localhost/web/pkg/noon_web_bg.wasm", 1_000_300, 1_000_000, 1_000_000, 100),
          entry(
            "https://cdn.jsdelivr.net/pyodide/v314.0.5/full/pyodide.asm.wasm",
            2_000_300,
            2_000_000,
            2_000_000,
            200,
          ),
        ],
      },
      {
        name: "engine-0",
        role: "engine",
        entries: [entry("http://localhost/web/pkg/noon_web_bg.wasm", 0, 1_000_000, 1_000_000, 80)],
      },
      {
        name: "render-0",
        role: "render",
        entries: [entry("http://localhost/web/pkg/noon_web_bg.wasm", 0, 1_000_000, 1_000_000, 70)],
      },
    ],
    { noonWasmPackageBytes: 1_000_000 },
  );

  assert.equal(summary.contextCount, 4);
  assert.equal(summary.requestCount, 5);
  assert.equal(summary.wasmRequestCount, 4);
  assert.equal(summary.noonWasm.requestCount, 3);
  assert.equal(summary.noonWasm.observedOwnerCount, 3);
  assert.deepEqual(summary.noonWasm.owners, ["authoring-0", "engine-0", "render-0"]);
  assert.equal(summary.noonWasm.packageBytes, 1_000_000);
  assert.equal(summary.noonWasm.packageBytesAcrossObservedOwners, 3_000_000);
  assert.equal(summary.noonWasm.transferBytes, 1_000_300);
  assert.equal(summary.noonWasm.encodedBodyBytes, 3_000_000);
  assert.equal(summary.transferBytes, 3_001_800);
  assert.equal(summary.encodedBodyBytes, 5_000_900);
  assert.equal(summary.contexts.find(({ role }) => role === "authoring").wasmRequests, 2);
  assert.equal(summary.largestResources[0].kind, "pyodide-wasm");
});

test("resource footprint rejects missing package size and malformed timing fields", () => {
  const contexts = [
    {
      name: "page",
      role: "page",
      entries: [
        {
          name: "http://localhost/main.js",
          initiatorType: "script",
          transferSize: 1,
          encodedBodySize: 1,
          decodedBodySize: 1,
          duration: 1,
        },
      ],
    },
  ];
  assert.throws(() => summarizeResourceFootprint(contexts, {}), /package bytes/);
  contexts[0].entries[0].transferSize = Number.NaN;
  assert.throws(
    () => summarizeResourceFootprint(contexts, { noonWasmPackageBytes: 1 }),
    /transferSize/,
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
