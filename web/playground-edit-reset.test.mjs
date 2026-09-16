import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");

test("editing or resetting Python source leaves the current preview running", () => {
  const inputStart = main.indexOf('sceneSourceEditor.addEventListener("input"');
  const inputEnd = main.indexOf('window.addEventListener("popstate"', inputStart);
  assert.ok(inputStart >= 0 && inputEnd > inputStart, "editor input handler must exist");
  const input = main.slice(inputStart, inputEnd);
  assert.match(input, /drafts\.set\(example\.id, sceneSourceEditor\.value\)/);
  assert.match(input, /current preview continues · Run to apply/);
  assert.doesNotMatch(
    input,
    /invalidateRun|terminate\(|discardEarlyContinuationRuntime|resetViewportForSourceEdit|setRuntimeStatus/,
    "typing must change only draft/UI state and must not mutate the active runtime",
  );

  const resetButtonStart = main.indexOf('resetButton.addEventListener("click"');
  const resetButtonEnd = main.indexOf('sceneSourceEditor.addEventListener(\n  "focus"', resetButtonStart);
  assert.ok(resetButtonStart >= 0 && resetButtonEnd > resetButtonStart);
  const resetButton = main.slice(resetButtonStart, resetButtonEnd);
  assert.match(resetButton, /sceneSourceEditor\.value = canonicalSource/);
  assert.match(resetButton, /current preview continues · Run to apply/);
  assert.doesNotMatch(
    resetButton,
    /invalidateRun|terminate\(|discardEarlyContinuationRuntime|resetViewportForSourceEdit|setRuntimeStatus/,
    "resetting source text must not stop the preview either",
  );
});

test("explicit Run is the boundary that supersedes a source-owned animation", () => {
  const supersedeStart = main.indexOf("async function supersedeActiveSourceContinuation()");
  const requestStart = main.indexOf("async function requestSceneRun()", supersedeStart);
  const requestEnd = main.indexOf("function sameSemanticContinuation(", requestStart);
  assert.ok(supersedeStart >= 0 && requestStart > supersedeStart && requestEnd > requestStart);

  const supersede = main.slice(supersedeStart, requestStart);
  assert.match(supersede, /generations\.invalidateRun\(\)/);
  assert.match(supersede, /cancelSemanticContinuation\(/);
  assert.match(supersede, /Superseded by an explicit playground Run/);
  assert.match(supersede, /discardEarlyContinuationRuntime\(continuation\.attachedPlayer\)/);

  const request = main.slice(requestStart, requestEnd);
  assert.match(request, /const priorRun = sceneRunPromise/);
  assert.match(request, /await supersedeActiveSourceContinuation\(\)/);
  assert.match(request, /await priorRun/);
  assert.match(request, /return runScene\(\)/);

  const busyStart = main.indexOf("function setBusy(busy)");
  const busyEnd = main.indexOf("function beginBusy()", busyStart);
  const busy = main.slice(busyStart, busyEnd);
  assert.match(busy, /activeSourceContinuation !== null/);
  assert.match(busy, /sceneButton\.disabled = busy && !canSupersedeActiveAnimation/);
});

test("metrics and status polling ignore a player retired by an explicit rerun", () => {
  const metricsStart = main.indexOf("async function updateWorkerMetrics()");
  const metricsEnd = main.indexOf("function stopMetricsPolling()", metricsStart);
  assert.ok(metricsStart >= 0 && metricsEnd > metricsStart, "metrics boundary must exist");
  const metrics = main.slice(metricsStart, metricsEnd);

  assert.match(metrics, /const activePlayer = player/);
  assert.match(metrics, /Promise\.all\(\[[\s\S]*activePlayer\.metrics\(\),[\s\S]*activePlayer\.state\(\)/);
  assert.match(metrics, /if \(player !== activePlayer\) return;/);
  assert.match(metrics, /setPlaybackRuntimeStatus\(\s*playbackState,/);
  assert.match(metrics, /if \(player === activePlayer && !playerNeedsRestart\)/);
});
