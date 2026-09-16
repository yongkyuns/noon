import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");

test("editing source invalidates the active run and replaces every viewport owner", () => {
  const resetStart = main.indexOf("function resetViewportForSourceEdit()");
  const resetEnd = main.indexOf("function sameSemanticContinuation(", resetStart);
  assert.ok(resetStart >= 0 && resetEnd > resetStart, "source-edit reset boundary must exist");
  const reset = main.slice(resetStart, resetEnd);

  assert.match(reset, /sceneRunPromise !== null/);
  assert.match(reset, /generations\.invalidateRun\(\)/);
  assert.match(reset, /stopMetricsPolling\(\)/);
  assert.match(reset, /playbackControls\?\.destroy\(\);\s*playbackControls = null;/);
  assert.match(
    reset,
    /const activePlayer = player;\s*player = null;[\s\S]*activePlayer\.terminate\(\);[\s\S]*adoptRuntimeCanvas\(activePlayer\)/,
    "published playback must lose ownership before its worker teardown can report stale failures",
  );
  assert.match(
    reset,
    /const preparation = runtimePreparation;\s*runtimePreparation = null;[\s\S]*preparation\.candidate\.terminate\(\);[\s\S]*adoptRuntimeCanvas\(preparation\.candidate\)/,
    "an unpublished prepared render owner must also be cancelled and replace its transferred canvas",
  );
  assert.match(reset, /status\.dataset\.executionTopology = "deferred-until-run"/);
});

test("typing or resetting source clears a live preview instead of letting stale animation continue", () => {
  const inputStart = main.indexOf('sceneSourceEditor.addEventListener("input"');
  const inputEnd = main.indexOf('window.addEventListener("popstate"', inputStart);
  assert.ok(inputStart >= 0 && inputEnd > inputStart, "editor input handler must exist");
  const input = main.slice(inputStart, inputEnd);
  assert.match(input, /player !== null \|\| runtimePreparation !== null \|\| sceneRunPromise !== null/);
  assert.match(input, /resetViewportForSourceEdit\(\)/);
  assert.match(input, /setRuntimeStatus\("Edited · preview reset", "ready"\)/);
  assert.match(input, /current run discarded · Run to replay/);

  const resetButtonStart = main.indexOf('resetButton.addEventListener("click"');
  const resetButtonEnd = main.indexOf('sceneSourceEditor.addEventListener(\n  "focus"', resetButtonStart);
  assert.ok(resetButtonStart >= 0 && resetButtonEnd > resetButtonStart);
  const resetButton = main.slice(resetButtonStart, resetButtonEnd);
  assert.match(resetButton, /resetViewportForSourceEdit\(\)/);
  assert.match(resetButton, /Run to replay/);
});

test("metrics and status polling ignore a player that an edit has already retired", () => {
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
