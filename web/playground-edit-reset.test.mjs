import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");

test("editing and Reset stop and restart through one source lifecycle boundary", () => {
  const inputStart = main.indexOf('sceneSourceEditor.addEventListener("input"');
  const inputEnd = main.indexOf('window.addEventListener("popstate"', inputStart);
  assert.ok(inputStart >= 0 && inputEnd > inputStart);
  const input = main.slice(inputStart, inputEnd);
  assert.match(input, /drafts\.set\(example\.id, sceneSourceEditor\.value\)/);
  assert.match(input, /sourceRestart\.edited\(/);
  assert.doesNotMatch(input, /invalidateRun|terminate\(|discardEarlyContinuationRuntime|setRuntimeStatus/);

  const resetStart = main.indexOf('resetButton.addEventListener("click"');
  const resetEnd = main.indexOf('sceneSourceEditor.addEventListener(\n  "focus"', resetStart);
  assert.ok(resetStart >= 0 && resetEnd > resetStart);
  const reset = main.slice(resetStart, resetEnd);
  assert.match(reset, /sceneSourceEditor\.value = canonicalSource/);
  assert.match(reset, /sourceRestart\.edited\(/);
  assert.doesNotMatch(reset, /invalidateRun|terminate\(|discardEarlyContinuationRuntime|setRuntimeStatus/);
});

test("explicit Run invalidates before cancelling a source continuation", () => {
  const start = main.indexOf("async function supersedeActiveSourceContinuation()");
  const end = main.indexOf("function sameSemanticContinuation(", start);
  assert.ok(start >= 0 && end > start);
  const lifecycle = main.slice(start, end);
  assert.match(lifecycle, /generations\.invalidateRun\(\)[\s\S]*cancelSemanticContinuation\(/);
  assert.match(lifecycle, /Superseded by a newer playground source run/);
  assert.match(lifecycle, /discardEarlyContinuationRuntime\(continuation\.attachedPlayer\)/);
  assert.match(main, /createRunRequestRouter\([\s\S]*run: runScene,[\s\S]*supersede: supersedeActiveSourceContinuation,/);
});

test("metric polling ignores a runtime replaced during its requests", () => {
  const start = main.indexOf("async function updateWorkerMetrics()");
  const end = main.indexOf("function stopMetricsPolling()", start);
  assert.ok(start >= 0 && end > start);
  const metrics = main.slice(start, end);
  assert.match(metrics, /const activePlayer = player/);
  assert.match(metrics, /Promise\.all\(\[[\s\S]*activePlayer\.metrics\(\),[\s\S]*activePlayer\.state\(\)/);
  assert.match(metrics, /if \(player !== activePlayer\) return;/);
  assert.match(metrics, /setPlaybackRuntimeStatus\(\s*playbackState,/);
});

test("the public gallery Run API uses the same explicit supersession boundary", () => {
  const apiStart = main.indexOf("window.__noonExampleGallery = {");
  const apiEnd = main.indexOf("window.addEventListener(\n    \"pagehide\"", apiStart);
  assert.ok(apiStart >= 0 && apiEnd > apiStart);
  const api = main.slice(apiStart, apiEnd);
  assert.match(api, /async run\(\) \{\s*return requestSceneRun\(\);\s*\}/);
});


test("progress is observed during source playback, with stale-generation and poll-epoch guards", () => {
  const metrics = main.slice(main.indexOf("async function updateWorkerMetrics()"), main.indexOf("try {\n  const requested ="));
  assert.doesNotMatch(metrics, /sceneRunPromise !== null/);
  assert.match(metrics, /busyDepth > 0 && activeSourceContinuation === null/);
  assert.match(metrics, /generations\.diagnostics\.runGeneration !== runGeneration/);
  assert.match(metrics, /playbackControls\?\.observe\(playbackState\)/);
  assert.match(metrics, /epoch === metricsEpoch/);
  const stop = main.slice(main.indexOf("function stopForSourceEdit()"), main.indexOf("function sameSemanticContinuation("));
  assert.ok(stop.indexOf("generations.invalidateRun()") < stop.indexOf("supersedeActiveSourceContinuation()"));
  assert.match(stop, /discardEarlyContinuationRuntime\(player\)/);
  assert.match(stop, /setTimeout\(retireAuthoring, 1000\)/);
  assert.match(stop, /Promise\.all\(\[cancellation, priorRun\]\)/);
});
