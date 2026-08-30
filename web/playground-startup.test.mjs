import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const worker = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");

const createRuntimeStart = main.indexOf("function createRuntimeClient()");
const preparationStart = main.indexOf("function ensureRuntimePreparation()", createRuntimeStart);
const runtimeReadyStart = main.indexOf("async function ensureRuntimeReady({", preparationStart);
const executionReadyStart = main.indexOf("async function ensureExecutionReady()", runtimeReadyStart);
assert.ok(
  createRuntimeStart >= 0 &&
    preparationStart > createRuntimeStart &&
    runtimeReadyStart > preparationStart &&
    executionReadyStart > runtimeReadyStart,
  "on-demand preparation and authored-engine startup boundaries must exist in order",
);
const createRuntimeBody = main.slice(createRuntimeStart, preparationStart);
const preparationBody = main.slice(preparationStart, runtimeReadyStart);
const runtimeReadyBody = main.slice(runtimeReadyStart, executionReadyStart);
assert.match(
  createRuntimeBody,
  /new AuthoringExecutionClient\(canvas,/,
  "first Run must create the GPU execution owner on demand",
);
assert.match(
  preparationBody,
  /const ready = candidate\.prepare\(\);/,
  "first Run must prepare the mode-free render owner before engine selection",
);
assert.doesNotMatch(
  preparationBody,
  /candidate\.start(?:Retained)?\(/,
  "mode-free preparation must not speculate a legacy or retained engine",
);
assert.match(
  preparationBody,
  /if \(runtimePreparation !== null\) return runtimePreparation;/,
  "failed or stale Python runs must be able to reuse an already prepared candidate",
);
assert.doesNotMatch(
  runtimeReadyBody,
  /warmAuthoringClient\(/,
  "execution startup must not bootstrap Python after the authored scene is already available",
);
assert.match(
  runtimeReadyBody,
  /const prepared = preparation \?\? ensureRuntimePreparation\(\);/,
  "authored startup must consume the candidate prepared in parallel with Python",
);
assert.match(
  runtimeReadyBody,
  /await prepared\.ready;/,
  "authored engine attachment must wait for render-owner preparation",
);
assert.match(
  runtimeReadyBody,
  /await nextPlayer\.startRetained\(sceneJson, retainedDocumentJson,/,
  "retained first runs must attach directly in retained mode",
);
assert.match(
  runtimeReadyBody,
  /await nextPlayer\.start\(sceneJson,/,
  "geometry-only first runs must attach their authored scene directly",
);
assert.doesNotMatch(
  runtimeReadyBody,
  /\{\\"version\\":1,\\"objects\\":\[\],\\"tracks\\":\[\]\}/,
  "cold startup must not boot a throwaway empty legacy scene",
);
const inFlightGuard = runtimeReadyBody.indexOf("if (runtimeStartPromise !== null)");
const livePlayerGuard = runtimeReadyBody.indexOf("if (player !== null)");
assert.ok(
  inFlightGuard >= 0 && livePlayerGuard > inFlightGuard,
  "concurrent startup callers must await the in-flight startup before observing the published player",
);
const playerPublish = runtimeReadyBody.indexOf("player = nextPlayer;");
const retainedStart = runtimeReadyBody.indexOf("await nextPlayer.startRetained");
const legacyStart = runtimeReadyBody.indexOf("await nextPlayer.start(sceneJson");
assert.ok(
  playerPublish > retainedStart && playerPublish > legacyStart,
  "the execution owner must publish only after the selected engine mode is ready",
);
assert.match(
  runtimeReadyBody,
  /if \(runtimePreparation === prepared\) \{\s*runtimePreparation = null;/,
  "successful engine publication must consume the prepared candidate exactly once",
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
const preparationCall = runSceneBody.indexOf(
  "const preparation = player === null ? ensureRuntimePreparation() : null;",
);
const authoringCall = runSceneBody.indexOf("authored = await client.run(source,");
const ensureRuntimeCall = runSceneBody.indexOf("result = await ensureRuntimeReady({");
const ensureExecutionCall = runSceneBody.indexOf("await ensureExecutionReady();");
const reconcileCall = runSceneBody.indexOf("result = await player.reconcileScene(sceneJson,");
assert.ok(authoringCall >= 0, "Run must author the selected Python source");
assert.ok(
  preparationCall >= 0 && preparationCall < authoringCall,
  "cold Run must kick render/WASM preparation before awaiting Python authoring",
);
assert.ok(
  ensureRuntimeCall > authoringCall,
  "engine selection and attachment must still wait until authoring identifies the required mode",
);
assert.match(
  runSceneBody,
  /result = await ensureRuntimeReady\(\{\s*preparation,/,
  "cold authored startup must consume the preparation kicked off before Python",
);
assert.ok(
  ensureExecutionCall > authoringCall && reconcileCall > ensureExecutionCall,
  "warm runs must preserve recovery before incremental reconciliation",
);
assert.match(
  runSceneBody,
  /const startRetained = \(authored\.retainedDocument\?\.objects\?\.length \?\? 0\) > 0;/,
  "first-run engine selection must derive from the authored retained sidecar",
);

const bootStart = main.indexOf("try {\n  const requested = requestedExampleId();");
assert.notEqual(bootStart, -1, "playground boot boundary must exist");
const bootCatch = main.indexOf("} catch (error) {\n  showError(error);\n}", bootStart);
assert.ok(bootCatch > bootStart, "playground boot boundary must terminate cleanly");
const bootBody = main.slice(bootStart, bootCatch);
assert.doesNotMatch(
  bootBody,
  /ensureAuthoringClient\(|ensureRuntimePreparation\(|new AuthoringExecutionClient\(|\.start\(.*objects.*tracks/,
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
  /pagehide[\s\S]*stopMetricsPolling\(\)[\s\S]*authoringClient\?\.terminate\(\)[\s\S]*preparation\?\.candidate\.terminate\(\)[\s\S]*player\?\.terminate\(\)/,
  "page teardown must release metrics, Python, unpublished preparation, and execution resources",
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
  "✓ playground defers page-load work, overlaps first-Run Python/render preparation, and selects the authored engine only after authoring",
);
