import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const mainSource = await readFile(new URL("./main.js", import.meta.url), "utf8");

function sourceBetween(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.ok(start >= 0 && end > start, `expected source boundary ${startMarker}`);
  return source.slice(start, end);
}

const runScene = sourceBetween(
  mainSource,
  "async function runScene()",
  "\nasync function selectExample(",
);
const runtimeReady = sourceBetween(
  mainSource,
  "async function ensureRuntimeReady(",
  "\nasync function ensureExecutionReady()",
);

const authoring = runScene.indexOf("authored = await client.run(source");
const runtimeStart = runScene.indexOf("result = await ensureRuntimeReady({");
assert.ok(authoring >= 0, "first run must author the selected source");
assert.ok(runtimeStart > authoring, "execution startup must not serialize source authoring behind an empty runtime");

assert.match(
  runScene,
  /const startRetained = semanticExecution === null && sceneSpecJson !== null;/u,
  "the authored canonical SceneSpec must select retained startup",
);
assert.match(
  runScene,
  /ensureRuntimeReady\(\{[\s\S]*sceneJson,[\s\S]*sceneSpecJson,[\s\S]*startRetained,/u,
  "first-run mode selection must be passed into runtime startup",
);

const retainedBranch = runtimeReady.match(
  /: startRetained\s*\?([\s\S]*?)\s*:\s*await nextPlayer\.start\(sceneJson/u,
);
assert.ok(retainedBranch, "runtime startup must branch between retained and legacy modes");
assert.match(
  retainedBranch[1],
  /await nextPlayer\.startRetainedCanonical\(sceneSpecJson,/u,
  "a retained first run must start the retained engine directly from the canonical SceneSpec",
);
assert.match(
  runtimeReady,
  /semanticExecution !== null[\s\S]*await nextPlayer\.startSemanticExecution\(semanticExecution,/u,
  "semantic results must start their context-held engine before inspecting compatibility documents",
);
assert.doesNotMatch(
  retainedBranch[1],
  /nextPlayer\.start\(sceneJson/u,
  "a retained first run must not bootstrap a legacy engine before retained startup",
);

console.log("✓ retained first run authors before startup and boots only the retained engine");
