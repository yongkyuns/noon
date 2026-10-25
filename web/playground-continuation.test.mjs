import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const runStart = main.indexOf("async function runScene()");
const runEnd = main.indexOf("async function selectExample(", runStart);
const runScene = main.slice(runStart, runEnd);
const adoptionStart = runScene.lastIndexOf("if (earlyContinuation !== null)");
const ordinaryStart = runScene.indexOf("else if (player === null)", adoptionStart);
assert.ok(adoptionStart >= 0 && ordinaryStart > adoptionStart);
const adoption = runScene.slice(adoptionStart, ordinaryStart);
const preparedStart = adoption.indexOf("if (semanticExecution.preparedPlayback === true)");
const sourceStart = adoption.indexOf("} else {", preparedStart);
const freshnessStart = adoption.indexOf("if (!isCurrentRun(runToken))", sourceStart);
assert.ok(preparedStart >= 0 && sourceStart > preparedStart && freshnessStart > sourceStart);
const sourceAdoption = adoption.slice(sourceStart, freshnessStart);

test("source-owned continuation attaches early and promotes the same semantic context to replay", () => {
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

  assert.match(adoption, /sameSemanticContinuation/);
  assert.match(adoption, /contextId: semanticExecution\.contextId/);
  assert.match(adoption, /callbackSessionId: semanticExecution\.callbackSessionId \?\? null/);
  assert.match(adoption, /continuationGeneration: null/);
  assert.match(adoption, /await measureReconciliation\("continuation-replay", \(\) => player\.reconcileSemanticExecution\(replayExecution,[\s\S]*?loopDurationSeconds,/);
  // Only the source-owned handoff must reconcile instead of observing status.
  // Prepared completion deliberately reads its already-adopted terminal metadata.
  assert.doesNotMatch(sourceAdoption, /await player\.state\(\)/);
  assert.match(sourceAdoption, /player\.reconcileSemanticExecution\(replayExecution,/);
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
  assert.match(runtime, /semanticExecution\.continuationGeneration != null &&\s*initialState\.preparedPlayback !== true/);
  assert.match(runtime, /updatePlaybackControls\(\{\s*supported: !sourceOwnsExecution && initialState\.replaySupported !== false,/);
  assert.match(
    runScene,
    /updatePlaybackControls\(\{\s*supported: result\.preparedPlayback === true,[\s\S]*?Python source continuing/,
  );
  assert.match(runScene, /continuationGeneration: null,[\s\S]*?updatePlaybackControls\(\{\s*supported: result\.replaySupported !== false,/);
  assert.match(runScene, /patchStatus\.dataset\.runGeneration = String\(runToken\.runGeneration\)/);
});

test("source-owned playback reports Playing even when the attached runtime is paused", () => {
  const presentationStart = main.indexOf("function setPlaybackRuntimeStatus(");
  const presentationEnd = main.indexOf("function setBusy(", presentationStart);
  const presentation = main.slice(presentationStart, presentationEnd);
  assert.match(presentation, /sceneRunPromise !== null && activeSourceContinuation !== null &&\s*playbackState\?\.preparedPlayback !== true/);
  assert.match(presentation, /sourceOwnsPlayback[\s\S]*\{ label: "Playing", state: "running" \}[\s\S]*playbackPresentation\(playbackState\)/);
});

// Execute the complete production completion branch, not a rewritten decision.
// The endpoint is doubled here; actual worker/render qualification is separate.
const identityStart = main.indexOf("function sameSemanticContinuation(");
assert.ok(identityStart >= 0 && identityStart < runStart);
const identity = main.slice(identityStart, runStart);
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const executeAdoption = new AsyncFunction("env", `
  const { authored, semanticExecution, player, client, loopDurationSeconds, runToken,
    isCurrentRun, recordStale, discardEarlyContinuationRuntime, measureReconciliation } = env;
  let earlyContinuation = env.continuation;
  let activeSourceContinuation = earlyContinuation;
  let result;
  ${identity}
  ${adoption}
  return { result, earlyContinuation, activeSourceContinuation };
`);

function completionFixture({ prepared = true, terminal = {}, returnedIdentity = {}, wrongPlayer = false,
  current = true } = {}) {
  const calls = [];
  const descriptor = { contextId: "context-7", continuationGeneration: 3, callbackSessionId: "callbacks-7",
    ...(prepared ? { preparedPlayback: true } : {}) };
  const state = { time: 0.75, playing: false, preparedPlayback: true,
    preparedFrontier: 4, durationSeconds: 4, buffering: false, ...terminal };
  const replayState = { time: 4, playing: false, replaySupported: true };
  const client = {};
  const player = {
    async state() { calls.push(["state"]); return state; },
    async reconcileSemanticExecution(next, options) {
      calls.push(["reconcile", next, options]); return replayState;
    },
    async seek() { assert.fail("completion must not seek"); },
    async resume() { assert.fail("completion must not resume"); },
    setInspectionZoom() { assert.fail("completion must not reconfigure inspection"); },
  };
  const semanticExecution = { ...descriptor, ...returnedIdentity };
  const env = {
    authored: { semanticExecution, duration: 4 }, semanticExecution,
    player, client, loopDurationSeconds: 4, runToken: {},
    continuation: { registration: { semanticExecution: descriptor }, attachedPlayer: wrongPlayer ? {} : player },
    isCurrentRun: () => current,
    recordStale: (_token, stage) => ({ stale: true, stage }),
    discardEarlyContinuationRuntime: owner => calls.push(["discard", owner]),
    measureReconciliation: async (phase, work) => { calls.push(["phase", phase]); return work(); },
  };
  return { calls, state, replayState, player, client, descriptor, env, run: () => executeAdoption(env) };
}

test("prepared completion observes terminal readiness without reconciliation, seek or resume", async () => {
  const f = completionFixture();
  const before = { ...f.state };
  const outcome = await f.run();
  assert.equal(outcome.result, f.state);
  assert.deepEqual(f.state, before, "keep the selected time and pause state");
  assert.equal(f.env.player, f.env.continuation.attachedPlayer);
  assert.deepEqual(f.calls, [["state"]]);
  assert.equal(outcome.earlyContinuation, null);
  assert.equal(outcome.activeSourceContinuation, null);
});

test("source-owned completion reconciles exactly once with the matching context and callbacks", async () => {
  const f = completionFixture({ prepared: false });
  const outcome = await f.run();
  assert.equal(outcome.result, f.replayState);
  assert.deepEqual(f.calls, [
    ["phase", "continuation-replay"],
    ["reconcile", { contextId: f.descriptor.contextId, callbackSessionId: f.descriptor.callbackSessionId,
      continuationGeneration: null }, { authoringClient: f.client, loopDurationSeconds: 4 }],
  ]);
  assert.equal(outcome.earlyContinuation, null);
  assert.equal(outcome.activeSourceContinuation, null);
});

for (const [label, terminal] of [
  ["unfinished history", { durationSeconds: null }],
  ["different final duration", { durationSeconds: 5 }],
  ["different endpoint profile", { preparedPlayback: false }],
]) {
  test(`prepared completion rejects ${label} without promoting source state`, async () => {
    const f = completionFixture({ terminal });
    await assert.rejects(f.run(), /no matching adopted terminal history/);
    assert.deepEqual(f.calls, [["state"]]);
  });
}

for (const [label, returnedIdentity] of [
  ["context", { contextId: "context-8" }],
  ["generation", { continuationGeneration: 4 }],
]) {
  test(`completion rejects mismatched ${label} before reading or reconciling`, async () => {
    const f = completionFixture({ returnedIdentity });
    await assert.rejects(f.run(), /does not match its early semantic continuation/);
    assert.deepEqual(f.calls, [["discard", f.player]]);
  });
}

test("completion cannot adopt through a replacement player", async () => {
  const f = completionFixture({ wrongPlayer: true });
  await assert.rejects(f.run(), /runtime changed before final adoption/);
  assert.deepEqual(f.calls, []);
});

test("a superseded completion observation is dropped instead of publishing readiness", async () => {
  const f = completionFixture({ current: false });
  assert.deepEqual(await f.run(), { stale: true, stage: "after-continuation-replay" });
  assert.deepEqual(f.calls, [["state"]]);
});
