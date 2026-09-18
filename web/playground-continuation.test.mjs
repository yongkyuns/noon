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
  assert.match(adoption, /contextId: semanticExecution\.contextId/);
  assert.match(adoption, /callbackSessionId: semanticExecution\.callbackSessionId \?\? null/);
  assert.match(adoption, /continuationGeneration: null/);
  assert.match(adoption, /await player\.reconcileSemanticExecution\(replayExecution,[\s\S]*?loopDurationSeconds,/);
  assert.doesNotMatch(adoption, /await player\.state\(\)/);
});

test("playground retains one cancellable source-continuation owner for explicit rerun", () => {
  assert.match(
    runScene,
    /discardEarlyContinuationRuntime\(earlyContinuation\?\.attachedPlayer\)/,
  );
  assert.match(
    runScene,
    /earlyContinuation = \{ registration, attachedPlayer, result, client, runToken \};\s*activeSourceContinuation = earlyContinuation;/,
  );
  assert.match(
    runScene,
    /if \(activeSourceContinuation\?\.runToken === runToken\) \{\s*activeSourceContinuation = null;/,
  );

  const discardStart = main.indexOf("function discardEarlyContinuationRuntime(");
  const discardEnd = main.indexOf("async function supersedeActiveSourceContinuation()", discardStart);
  const discard = main.slice(discardStart, discardEnd);
  assert.match(
    discard,
    /if \(attachedPlayer == null\) return;[\s\S]*attachedPlayer\.terminate\(\);[\s\S]*if \(player !== attachedPlayer\) return;[\s\S]*adoptRuntimeCanvas\(attachedPlayer\);[\s\S]*player = null;/,
    "stale continuation teardown must publish the fresh replacement canvas before clearing its owner",
  );
});

test("source-owned first pass keeps playback controls unavailable until replay is ready", () => {
  const runtimeStart = main.indexOf("async function ensureRuntimeReady(");
  const runtimeEnd = main.indexOf("async function ensureExecutionReady(", runtimeStart);
  const runtime = main.slice(runtimeStart, runtimeEnd);
  assert.match(runtime, /semanticExecution\.continuationGeneration != null/);
  assert.match(runtime, /updatePlaybackControls\(\{\s*supported: !sourceOwnsExecution && initialState\.replaySupported !== false,/);
  assert.match(
    runScene,
    /updatePlaybackControls\(\{\s*supported: false,[\s\S]*?Python source continuing/,
  );
  assert.match(runScene, /continuationGeneration: null,[\s\S]*?updatePlaybackControls\(\{\s*supported: result\.replaySupported !== false,/);
  assert.match(runScene, /patchStatus\.dataset\.runGeneration = String\(runToken\.runGeneration\)/);
});

test("source-owned playback reports Playing even when the attached runtime is paused", () => {
  const presentationStart = main.indexOf("function setPlaybackRuntimeStatus(");
  const presentationEnd = main.indexOf("function setBusy(", presentationStart);
  const presentation = main.slice(presentationStart, presentationEnd);
  assert.match(presentation, /sceneRunPromise !== null && activeSourceContinuation !== null/);
  assert.match(presentation, /sourceOwnsPlayback[\s\S]*\{ label: "Playing", state: "running" \}[\s\S]*playbackPresentation\(playbackState\)/);
});
