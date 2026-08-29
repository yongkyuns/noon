import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const worker = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");

const runtimeReadyStart = main.indexOf("async function ensureRuntimeReady()");
const executionReadyStart = main.indexOf("async function ensureExecutionReady()", runtimeReadyStart);
assert.ok(
  runtimeReadyStart >= 0 && executionReadyStart > runtimeReadyStart,
  "on-demand runtime startup boundary must exist",
);
const runtimeReadyBody = main.slice(runtimeReadyStart, executionReadyStart);
assert.match(
  runtimeReadyBody,
  /warmAuthoringClient\(\);/,
  "first runtime request must begin Python warmup",
);
assert.match(
  runtimeReadyBody,
  /new AuthoringExecutionClient\(canvas,/,
  "first runtime request must create the GPU execution owner",
);
assert.match(
  runtimeReadyBody,
  /await nextPlayer\.start\(/,
  "on-demand startup must await execution readiness before publishing controls",
);
const inFlightGuard = runtimeReadyBody.indexOf("if (runtimeStartPromise !== null)");
const livePlayerGuard = runtimeReadyBody.indexOf("if (player !== null)");
assert.ok(
  inFlightGuard >= 0 && livePlayerGuard > inFlightGuard,
  "concurrent startup callers must await the in-flight startup before observing the published player",
);
assert.match(
  runtimeReadyBody,
  /playerNeedsRestart = false;/,
  "successful fresh runtime startup must clear stale restart state",
);
assert.match(
  runtimeReadyBody,
  /status\.dataset\.runtimeStartup = "started-on-demand"/,
  "runtime startup must expose its on-demand state for browser diagnostics",
);

const runSceneStart = main.indexOf("async function runScene()");
const selectExampleStart = main.indexOf("async function selectExample(", runSceneStart);
assert.ok(runSceneStart >= 0 && selectExampleStart > runSceneStart, "runScene boundary must exist");
const runSceneBody = main.slice(runSceneStart, selectExampleStart);
const ensureRuntimeCall = runSceneBody.indexOf("await ensureRuntimeReady();");
const ensureExecutionCall = runSceneBody.indexOf("await ensureExecutionReady();");
assert.ok(ensureRuntimeCall >= 0, "Run must start the deferred runtime");
assert.ok(
  ensureExecutionCall > ensureRuntimeCall,
  "runtime startup must complete before execution recovery/reconciliation",
);

const bootStart = main.indexOf("try {\n  const requested = requestedExampleId();");
assert.notEqual(bootStart, -1, "playground boot boundary must exist");
const bootCatch = main.indexOf("} catch (error) {\n  showError(error);\n}", bootStart);
assert.ok(bootCatch > bootStart, "playground boot boundary must terminate cleanly");
const bootBody = main.slice(bootStart, bootCatch);
assert.doesNotMatch(
  bootBody,
  /warmAuthoringClient\(|new AuthoringExecutionClient\(|\.start\(.*objects.*tracks/,
  "initial page boot must not create Python or GPU runtime resources",
);
assert.doesNotMatch(
  bootBody,
  /scheduleStartupAutoplay|(?:await|void) runScene\(\)/,
  "initial page boot must not autoplay a Python scene",
);
assert.match(
  bootBody,
  /await selectExample\(initialExample, \{ run: false \}\);/,
  "initial page boot must load only the selected source",
);
assert.match(
  bootBody,
  /status\.dataset\.runtimeStartup = "deferred"/,
  "initial page boot must expose deferred runtime state",
);
assert.match(
  bootBody,
  /window\.__noonExampleGallery =/,
  "gallery API must become available without waiting for Pyodide or GPU startup",
);

assert.doesNotMatch(
  main,
  /^void import\("\.\/python-editor\.js"\)/m,
  "CodeMirror/Ruff must not load eagerly at module startup",
);
const editorLoaderStart = main.indexOf("function loadEnhancedPythonEditor()");
assert.notEqual(editorLoaderStart, -1, "lazy Python editor loader must exist");
const editorImport = main.indexOf('import("./python-editor.js")', editorLoaderStart);
assert.ok(editorImport > editorLoaderStart, "Python editor import must live behind the lazy loader");
assert.match(
  main,
  /sceneSourceEditor\.addEventListener\([\s\S]*?"focus"[\s\S]*?loadEnhancedPythonEditor\(\)[\s\S]*?\{ once: true \}/,
  "enhanced Python editor must load only on first editor focus",
);

assert.match(main, /const GALLERY_PAGE_SIZE = 18;/, "gallery DOM residency must be bounded");
assert.match(
  main,
  /visible\.slice\(start, start \+ GALLERY_PAGE_SIZE\)/,
  "gallery rendering must materialize only the current page",
);
assert.match(main, /image\.fetchPriority = "low";/, "gallery thumbnails must stay low priority");
assert.match(
  main,
  /content-visibility: auto;/,
  "gallery cards must allow the browser to skip offscreen layout and paint work",
);
const setBusyStart = main.indexOf("function setBusy(busy)");
const beginBusyStart = main.indexOf("function beginBusy()", setBusyStart);
assert.ok(setBusyStart >= 0 && beginBusyStart > setBusyStart, "busy-control boundary must exist");
const setBusyBody = main.slice(setBusyStart, beginBusyStart);
assert.match(
  setBusyBody,
  /if \(busy\) \{\s*nextGalleryPage\.disabled = true;/,
  "busy scene work must disable forward gallery paging as well as the previous-page control",
);

assert.match(main, /const METRICS_POLL_MS = 500;/, "runtime metrics polling must be rate-limited");
assert.match(
  main,
  /metricsTimer = setTimeout\(poll, METRICS_POLL_MS\);/,
  "runtime metrics must use the bounded polling cadence",
);
assert.doesNotMatch(
  main,
  /requestAnimationFrame\(frame\)/,
  "runtime metrics must not schedule work on every animation frame",
);
assert.match(
  main,
  /document\.visibilityState === "hidden"/,
  "runtime metrics must stop doing work while the page is hidden",
);
assert.match(
  bootBody,
  /pagehide[\s\S]*stopMetricsPolling\(\)[\s\S]*authoringClient\?\.terminate\(\)[\s\S]*player\?\.terminate\(\)/,
  "page teardown must release metrics, Python, and execution resources",
);

const initializeStart = worker.indexOf("async function initializePyodide()");
assert.notEqual(initializeStart, -1, "Python worker initializer must exist");
const firstBarrier = worker.indexOf("await startupResourcesReady;", initializeStart);
assert.ok(firstBarrier > initializeStart, "startup resources must share one handled readiness barrier");
for (const kickoff of [
  "const noonWebReady = initNoonWeb();",
  "const pyodideReady = loadPyodide();",
  "const compatibilityBundleReady = loadCompatibilityBundle();",
  "const startupResourcesReady = Promise.all([",
]) {
  const position = worker.indexOf(kickoff, initializeStart);
  assert.ok(position > initializeStart, `missing startup kickoff: ${kickoff}`);
  assert.ok(position < firstBarrier, `${kickoff} must start before the first initialization barrier`);
}
assert.doesNotMatch(worker, /await initNoonWeb\(\)/, "Noon WASM must not serialize Pyodide startup");
assert.doesNotMatch(worker, /await loadPyodide\(\)/, "Pyodide must not serialize Noon WASM startup");
assert.doesNotMatch(
  worker,
  /await (?:noonWebReady|pyodideReady|compatibilityBundleReady)/,
  "independent startup promises must be handled by the shared barrier",
);
assert.match(
  worker,
  /const compatibilityBundleReady = loadCompatibilityBundle\(\);/,
  "compatibility source loading must remain parallel with WASM and Pyodide startup",
);

console.log(
  "✓ playground defers heavyweight startup, bounds gallery residency, and preserves parallel Python bootstrap",
);
