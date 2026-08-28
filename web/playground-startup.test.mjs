import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const worker = await readFile(new URL("./python-worker.js", import.meta.url), "utf8");

const startupRegion = main.indexOf("const EMPTY_SCENE_JSON");
assert.notEqual(startupRegion, -1, "playground startup region must exist");
const warmupCall = main.indexOf("warmAuthoringClient();", startupRegion);
const playerStart = main.indexOf("await player.start(", startupRegion);
assert.ok(warmupCall > startupRegion, "Python authoring warmup must start during playground boot");
assert.ok(
  warmupCall < playerStart,
  "Python authoring warmup must start before execution/render workers finish booting",
);
assert.match(
  main,
  /authoringClient !== client\) return;[\s\S]*client\.terminated[\s\S]*authoringClient = null/,
  "failed eager warmup must discard a dead Python client for the next Run",
);

const initialSelection = main.indexOf(
  "await selectExample(initialExample, { run: false });",
  startupRegion,
);
const galleryInstall = main.indexOf("window.__noonExampleGallery =", startupRegion);
const autoplaySchedule = main.indexOf(
  "scheduleStartupAutoplay(initialExample, startupAuthoringClient);",
  startupRegion,
);
assert.ok(initialSelection > playerStart, "initial source selection must follow renderer startup");
assert.ok(galleryInstall > initialSelection, "playground API must install after initial source selection");
assert.ok(
  autoplaySchedule > galleryInstall,
  "initial Python autoplay must be scheduled only after the playground is interactive",
);
assert.doesNotMatch(
  main.slice(startupRegion),
  /await selectExample\(initialExample, \{ run: true \}\)/,
  "playground startup must not await Python scene authoring",
);

const autoplayStart = main.indexOf("function scheduleStartupAutoplay(");
const executionReadyStart = main.indexOf("async function ensureExecutionReady()", autoplayStart);
assert.ok(autoplayStart >= 0 && executionReadyStart > autoplayStart, "startup autoplay helper must exist");
const autoplayBody = main.slice(autoplayStart, executionReadyStart);
assert.match(autoplayBody, /client\.ready\(\)\.then\(/, "autoplay must wait for warm Python readiness");
assert.match(autoplayBody, /selectedExampleId !== exampleId/, "autoplay must yield to a newer selection");
assert.match(autoplayBody, /sceneRunPromise !== null/, "autoplay must yield to a user-started run");
assert.match(
  autoplayBody,
  /sceneSourceEditor\.value !== canonicalSource/,
  "autoplay must yield to source edits",
);

const runSceneStart = main.indexOf("async function runScene()");
const selectExampleStart = main.indexOf("async function selectExample(", runSceneStart);
assert.ok(runSceneStart >= 0 && selectExampleStart > runSceneStart, "runScene boundary must exist");
assert.match(
  main.slice(runSceneStart, selectExampleStart),
  /cancelStartupAutoplay\(\);/,
  "an explicit run must cancel pending startup autoplay",
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
  "✓ playground becomes interactive before Python autoplay and overlaps bundled cold-start resources",
);
