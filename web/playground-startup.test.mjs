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

const initializeStart = worker.indexOf("async function initializePyodide()");
assert.notEqual(initializeStart, -1, "Python worker initializer must exist");
const firstBarrier = worker.indexOf("await startupResourcesReady;", initializeStart);
assert.ok(firstBarrier > initializeStart, "startup resources must share one handled readiness barrier");
for (const kickoff of [
  "const noonWebReady = initNoonWeb();",
  "const pyodideReady = loadPyodide();",
  "const compatibilitySourcesReady = Promise.all([",
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
  /await (?:noonWebReady|pyodideReady|compatibilitySourcesReady)/,
  "independent startup promises must be handled by the shared barrier",
);
assert.ok(
  (worker.match(/fetch\(new URL\("\.\/python\//g) ?? []).length >= 20,
  "compatibility source preload should cover the full Python compatibility surface",
);

console.log("✓ playground overlaps renderer, Pyodide, Noon WASM, and compatibility-source cold start");
