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
  "first authored Run must create the GPU execution owner on demand",
);
assert.match(
  preparationBody,
  /const ready = candidate\.prepare\(\);/,
  "first authored Run must prepare the mode-free render owner before engine selection",
);
assert.doesNotMatch(
  preparationBody,
  /candidate\.start(?:RetainedCanonical)?\(/,
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
  /await nextPlayer\.startRetainedCanonical\(sceneSpecJson,/,
  "retained first runs must attach directly from canonical SceneSpec",
);
assert.doesNotMatch(
  runtimeReadyBody,
  /startRetained\(sceneJson, retainedDocumentJson/,
  "normal retained startup must not reconstruct the transitional split payload",
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
const retainedStart = runtimeReadyBody.indexOf("await nextPlayer.startRetainedCanonical");
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
const reconcileCall = runSceneBody.indexOf("await player.reconcileScene(sceneJson,");
assert.ok(authoringCall >= 0, "Run must author the selected Python source");
assert.ok(
  preparationCall >= 0 && preparationCall < authoringCall,
  "cold authored Run must kick render/WASM preparation before awaiting Python authoring",
);
assert.ok(
  ensureRuntimeCall > authoringCall,
  "engine selection and attachment must wait until authoring identifies the required engine mode",
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
  /const startRetained = semanticExecution === null && sceneSpecJson !== null;/,
  "first-run retained engine selection must derive from canonical SceneSpec availability",
);
assert.doesNotMatch(
  runSceneBody,
  /authored\.retainedDocument|retainedDocumentJson/,
  "normal playground routing must not depend on the transitional retained sidecar",
);

const bootStart = main.indexOf("try {\n  const requested = requestedExampleId();");
assert.notEqual(bootStart, -1, "playground boot boundary must exist");
const bootCatch = main.indexOf("} catch (error) {\n  showError(error);\n}", bootStart);
assert.ok(bootCatch > bootStart, "playground boot boundary must terminate cleanly");
const bootBody = main.slice(bootStart, bootCatch);
assert.doesNotMatch(
  bootBody,
  /ensureAuthoringClient\(|ensureRuntimePreparation\(|new AuthoringExecutionClient\(|\.start\(.*objects.*tracks/,
  "initial main boot must not synchronously create Python or GPU runtime resources before the post-paint preload",
);
assert.doesNotMatch(
  bootBody,
  /scheduleStartupAutoplay|(?:await|void) runScene\(\)/,
  "initial main boot must not directly autoplay a Python scene before the post-paint preload",
);
assert.match(
  bootBody,
  /await selectExample\(initialExample, \{ run: false \}\);/,
  "initial main boot must load the selected source before preload begins",
);
assert.match(
  bootBody,
  /status\.dataset\.runtimeStartup = "deferred"/,
  "initial main boot must expose deferred runtime state until the post-paint preload starts",
);
assert.match(
  bootBody,
  /window\.__noonExampleGallery =/,
  "gallery API must become available without synchronously waiting for Pyodide or GPU startup",
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
const initializeBody = worker.slice(initializeStart, firstBarrier);
for (const [pattern, label] of [
  [
    /const noonWebReady = measureStartupTask\([\s\S]*?"noonWebInitMs"[\s\S]*?\(\) => initNoonWeb\(\)\);/,
    "Noon WASM startup",
  ],
  [
    /const pyodideReady = measureStartupTask\([\s\S]*?"pyodideInitMs"[\s\S]*?\(\) => loadPyodide\(\)\);/,
    "Pyodide startup",
  ],
  [
    /const compatibilityBundleReady = measureStartupTask\([\s\S]*?"compatibilityBundleMs"[\s\S]*?\(\) => loadCompatibilityBundle\(\),?[\s\S]*?\);/,
    "compatibility bundle startup",
  ],
  [/const startupResourcesReady = Promise\.all\(\[/, "shared startup barrier"],
]) {
  assert.match(initializeBody, pattern, `${label} must start before the first initialization barrier`);
}
assert.doesNotMatch(worker, /await initNoonWeb\(\)/, "Noon WASM must not serialize Pyodide startup");
assert.doesNotMatch(worker, /await loadPyodide\(\)/, "Pyodide must not serialize Noon WASM startup");
assert.doesNotMatch(
  worker,
  /await (?:noonWebReady|pyodideReady|compatibilityBundleReady)/,
  "independent startup promises must be handled by the shared barrier",
);
assert.match(
  initializeBody,
  /"compatibilityBundleMs"[\s\S]*?\(\) => loadCompatibilityBundle\(\)/,
  "compatibility source loading must remain parallel with WASM and Pyodide startup",
);

console.log(
  "✓ playground paints source first, post-paint preload reuses the existing authored Run path, and Python/render preparation remains overlapped",
);
