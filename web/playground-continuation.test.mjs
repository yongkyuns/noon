import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const runStart = main.indexOf("async function runScene()");
const runEnd = main.indexOf("async function selectExample(", runStart);
const runScene = main.slice(runStart, runEnd);

test("playground attaches an early Python continuation and adopts it only once", () => {
  assert.ok(runStart >= 0 && runEnd > runStart);
  assert.match(runScene, /onSemanticContinuation\(registration\)/);
  assert.match(
    runScene,
    /ensureRuntimeReady\(\{[\s\S]*semanticExecution: registration\.semanticExecution/,
  );
  assert.match(
    runScene,
    /player\.reconcileSemanticExecution\(\s*registration\.semanticExecution/,
  );

  const adoptionStart = runScene.lastIndexOf("if (earlyContinuation !== null)");
  const ordinaryStart = runScene.indexOf("else if (player === null)", adoptionStart);
  const adoption = runScene.slice(adoptionStart, ordinaryStart);
  assert.match(adoption, /sameSemanticContinuation/);
  assert.match(adoption, /await player\.state\(\)/);
  assert.doesNotMatch(adoption, /startSemanticExecution|reconcileSemanticExecution/);
});

test("playground tears down an early continuation when its run becomes stale", () => {
  assert.match(
    runScene,
    /discardEarlyContinuationRuntime\(earlyContinuation\?\.attachedPlayer\)/,
  );
  assert.match(
    runScene,
    /Python semantic continuation was superseded during startup/,
  );
  assert.match(
    runScene,
    /catch \(error\) \{\s*discardEarlyContinuationRuntime\(earlyContinuation\?\.attachedPlayer\)/,
  );
});

test("source-owned semantic runs do not expose unsupported playback controls", () => {
  const runtimeStart = main.indexOf("async function ensureRuntimeReady(");
  const runtimeEnd = main.indexOf("async function ensureExecutionReady(", runtimeStart);
  const runtime = main.slice(runtimeStart, runtimeEnd);
  assert.match(runtime, /semanticExecution\?\.continuationGeneration != null/);
  assert.match(runtime, /if \(!sourceOwnsExecution\) \{[\s\S]*new PlaygroundPlaybackControls/);
  assert.match(
    runScene,
    /if \(semanticExecution\?\.continuationGeneration != null\) \{[\s\S]*playbackControls = null;/,
  );
});
