import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const runStart = main.indexOf("async function runScene()");
const runEnd = main.indexOf("async function selectExample(", runStart);
const runScene = main.slice(runStart, runEnd);

test("playground attaches an early Python continuation then promotes the same semantic context to replay", () => {
  assert.ok(runStart >= 0 && runEnd > runStart);
  assert.match(runScene, /onSemanticContinuation\(registration\)/);
  assert.match(
    runScene,
    /ensureRuntimeReady\(\{[\s\S]*?semanticExecution: registration\.semanticExecution/,
  );
  assert.match(
    runScene,
    /player\.reconcileSemanticExecution\(\s*registration\.semanticExecution/,
  );

  const adoptionStart = runScene.lastIndexOf("if (earlyContinuation !== null)");
  const ordinaryStart = runScene.indexOf("else if (player === null)", adoptionStart);
  const adoption = runScene.slice(adoptionStart, ordinaryStart);
  assert.match(adoption, /sameSemanticContinuation/);
  assert.match(adoption, /contextId: semanticExecution\.contextId/);
  assert.match(adoption, /callbackSessionId: semanticExecution\.callbackSessionId \?\? null/);
  assert.match(adoption, /continuationGeneration: null/);
  assert.match(
    adoption,
    /await player\.reconcileSemanticExecution\(replayExecution,[\s\S]*?loopDurationSeconds,/,
    "a completed source-owned run must reattach the same semantic context as an ordinary replay lease",
  );
  assert.doesNotMatch(
    adoption,
    /await player\.state\(\)/,
    "final continuation adoption must not leave the source-owned endpoint as the playback owner",
  );
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

  const discardStart = main.indexOf("function discardEarlyContinuationRuntime(");
  const discardEnd = main.indexOf("function resetViewportForSourceEdit(", discardStart);
  const discard = main.slice(discardStart, discardEnd);
  assert.match(
    discard,
    /if \(attachedPlayer == null\) return;[\s\S]*attachedPlayer\.terminate\(\);[\s\S]*if \(player !== attachedPlayer\) return;[\s\S]*adoptRuntimeCanvas\(attachedPlayer\);[\s\S]*player = null;/,
    "stale continuation teardown must publish the fresh replacement canvas before clearing its owner",
  );
});

test("source-owned first pass keeps playback controls unavailable until the replay lease is ready", () => {
  const runtimeStart = main.indexOf("async function ensureRuntimeReady(");
  const runtimeEnd = main.indexOf("async function ensureExecutionReady(", runtimeStart);
  const runtime = main.slice(runtimeStart, runtimeEnd);
  assert.match(runtime, /semanticExecution\.continuationGeneration != null/);
  assert.match(
    runtime,
    /updatePlaybackControls\(\{\s*supported: !sourceOwnsExecution,/,
    "runtime startup must not expose controls on a source-owned continuation endpoint",
  );
  assert.match(
    runScene,
    /updatePlaybackControls\(\{\s*supported: false,[\s\S]*?Python source continuing/,
    "an already-live replay control surface must be removed when a new source continuation takes ownership",
  );
  assert.match(
    runScene,
    /continuationGeneration: null,[\s\S]*?reconcileSemanticExecution\(replayExecution,[\s\S]*?updatePlaybackControls\(\{\s*supported: true,/,
    "controls must become available only after the completed source is attached as an ordinary replay",
  );
});
