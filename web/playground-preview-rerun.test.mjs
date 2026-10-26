import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";
import { authoringSourceDocument } from "./authoring-diagnostic-runtime.js";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const runStart = main.indexOf("async function runScene()");
const runEnd = main.indexOf("const runRequestRouter =", runStart);
const discardStart = main.indexOf("function discardEarlyContinuationRuntime(");
const discardEnd = main.indexOf("async function supersedeActiveSourceContinuation()", discardStart);
assert.ok(runStart >= 0 && runEnd > runStart && discardStart >= 0 && discardEnd > discardStart);
const flush = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
function deferred() {
  let resolve, reject;
  const promise = new Promise((a, b) => { resolve = a; reject = b; });
  return { promise, resolve, reject };
}

// Exercise the actual Run/retirement functions with an already-completed source
// that is replaying. No input/edit event occurs before this explicit Run.
function fixture({ retire = () => Promise.resolve(), started = true } = {}) {
  const events = [], errors = [];
  const failure = new Error("second run fails");
  const oldCanvas = { style: {} };
  let playing = started, controllable = true, current = true, releases = 0, authorCalls = 0;
  const temporaryAttachment = {};
  let activeAttachment = started ? temporaryAttachment : null;
  const oldPlayer = {
    canvas: oldCanvas, terminations: 0,
    retireExecutionSource() {
      events.push("retire");
      assert.equal(oldCanvas.style.visibility, "hidden", "input must not reach a retiring source");
      return retire().then(() => { playing = false; activeAttachment = null; });
    },
    terminate() {
      events.push("terminate"); playing = false; this.terminations += 1;
      this.canvas = { style: {} };
    },
  };
  const client = { terminated: false, run() {
    assert.equal(activeAttachment, null, "new source must not mutate the arena before input retirement");
    events.push("author"); authorCalls += 1; return Promise.reject(failure);
  } };
  const context = {
    performance, canvas: oldCanvas, player: started ? oldPlayer : null, playerNeedsRestart: false,
    sceneRunPromise: null, activeSourceContinuation: null, activeRunRequest: null,
    selectedExampleId: "one", sceneSourceEditor: { value: "same source with a run-dependent failure" },
    status: { dataset: {} }, patchStatus: { dataset: {} }, authoringClient: client,
    currentExample: () => ({ id: "one", title: "One" }),
    generations: { beginRun: () => ({ selectionGeneration: 1, runGeneration: 2 }) },
    isCurrentRun: () => current, recordStale: (_, stage) => ({ stale: true, stage }),
    resetPresentationRate() {}, stopMetricsPolling() {},
    beginBusy: () => () => { releases += 1; }, setRuntimeStatus() {},
    authoringSourceDocument, diagnosticPanel: { beginRun() {} },
    previousPreview: { hold() { events.push("hold"); } },
    playbackControls: { setControllable(value) { controllable = value; }, destroy() { controllable = false; } },
    ensureAuthoringClient: () => client,
    ensureRuntimePreparation() { events.push("prepare"); return {}; },
    adoptRuntimeCanvas(owner) { context.canvas = owner.canvas; },
    showSceneError(error) { errors.push(error); }, showError(error) { errors.push(error); },
  };
  vm.runInNewContext(main.slice(discardStart, discardEnd) + main.slice(runStart, runEnd), context);
  return { context, events, errors, failure, oldPlayer, client,
    get playing() { return playing; }, get controllable() { return controllable; },
    get authorCalls() { return authorCalls; }, get releases() { return releases; },
    get activeAttachment() { return activeAttachment; }, temporaryAttachment,
    supersede() { current = false; } };
}

test("explicit Run retires active input before authoring while retaining its renderer on source failure", async () => {
  const f = fixture();
  const result = await f.context.runScene();
  assert.equal(result.stale, false);
  assert.equal(f.playing, false, "old replay must not run invisibly behind the frozen error preview");
  assert.equal(f.controllable, false);
  assert.deepEqual(f.events, ["hold", "retire", "author", "hold"]);
  assert.equal(f.activeAttachment, null);
  assert.equal(f.context.player, f.oldPlayer, "successful retirement keeps the warm renderer owner");
  assert.equal(f.oldPlayer.terminations, 0);
  assert.equal(f.client.terminated, false);
  assert.deepEqual(f.errors, [f.failure], "the original authoring failure must stay intact");
  assert.equal(f.context.status.dataset.playbackPlaying, "false");
  assert.equal(f.context.status.dataset.playbackControls, "unavailable");
  assert.equal(f.releases, 1);
});

test("a first Run still initializes normally without retiring an absent owner", async () => {
  const f = fixture({ started: false });
  await f.context.runScene();
  assert.deepEqual(f.events, ["hold", "prepare", "author", "hold"]);
  assert.equal(f.authorCalls, 1);
});

for (const synchronous of [false, true]) {
  test(`${synchronous ? "synchronous" : "asynchronous"} release rejection preserves the exact source and prevents authoring`, async () => {
    const rejection = new Error("canonical retirement rejected");
    const f = fixture({ retire() {
      if (synchronous) throw rejection;
      return Promise.reject(rejection);
    } });
    await f.context.runScene();
    assert.deepEqual(f.events, ["hold", "retire", "hold"]);
    assert.equal(f.authorCalls, 0);
    assert.equal(f.oldPlayer.terminations, 0);
    assert.equal(f.context.player, f.oldPlayer);
    assert.equal(f.activeAttachment, f.temporaryAttachment);
    assert.deepEqual(f.errors, [rejection]);
  });
}

for (const reject of [false, true]) {
  test(`superseded release ${reject ? "failure" : "completion"} cannot author old source or retire its successor`, async () => {
    const gate = deferred();
    const f = fixture({ retire: () => gate.promise });
    const run = f.context.runScene();
    assert.equal(f.authorCalls, 0);
    f.supersede();
    const successor = { terminate() { assert.fail("obsolete release must not retire a newer owner"); } };
    f.context.player = successor;
    if (reject) gate.reject(new Error("retired")); else gate.resolve();
    assert.equal((await run).stale, true);
    assert.equal(f.authorCalls, 0);
    assert.equal(f.oldPlayer.terminations, 0, "superseding lifecycle owns retirement, not the obsolete Run");
    assert.equal(f.context.player, successor);
    assert.deepEqual(f.errors, []);
    assert.equal(f.releases, 1);
  });
}

test("Run waits for canonical source-release acknowledgement before authoring and joins concurrent requests", async () => {
  const gate = deferred();
  const f = fixture({ retire: () => gate.promise });
  const first = f.context.runScene();
  const second = f.context.runScene();
  await flush();
  assert.deepEqual(f.events, ["hold", "retire"]);
  assert.equal(f.authorCalls, 0);
  assert.equal(f.controllable, false);
  gate.resolve();
  await Promise.all([first, second]);
  assert.equal(f.authorCalls, 1);
  assert.equal(f.releases, 1);
  assert.equal(f.context.sceneRunPromise, null);
});
