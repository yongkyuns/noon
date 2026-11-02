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
const hookStart = main.indexOf("async function runPlaygroundTestHook(");
const hookEnd = main.indexOf("async function loadDemoAuthoringSource(", hookStart);
const profileStart = main.indexOf("function isPreparedPlayback(");
const profileEnd = main.indexOf("function playbackPresentation(", profileStart);
assert.ok(runStart >= 0 && runEnd > runStart && discardStart >= 0 && discardEnd > discardStart);
assert.ok(hookStart >= 0 && hookEnd > hookStart && profileStart >= 0 && profileEnd > profileStart);
// Keep startup's real hook and endpoint-identity policy in the VM. Omitting a
// dependency turns a regression assertion into an unrelated ReferenceError.
const production = main.slice(hookStart, hookEnd) + main.slice(profileStart, profileEnd) +
  main.slice(discardStart, discardEnd) + main.slice(runStart, runEnd);
function deferred() {
  let resolve, reject;
  const promise = new Promise((a, b) => { resolve = a; reject = b; });
  return { promise, resolve, reject };
}

// Exercise the actual Run/retirement functions with an already-completed source
// that is replaying. No input/edit event occurs before this explicit Run.
function fixture({ retire = () => Promise.resolve(), started = true, prepared = false,
  authoringRunOptions } = {}) {
  const events = [], errors = [], authorRequests = [];
  const retireEntered = deferred();
  const failure = new Error("second run fails");
  const oldCanvas = { style: {} };
  let playing = started, controllable = true, current = true, releases = 0, authorCalls = 0;
  const temporaryAttachment = {};
  let activeAttachment = started && !prepared ? temporaryAttachment : null;
  const oldPlayer = {
    canvas: oldCanvas, terminations: 0,
    retireExecutionSource() {
      events.push("retire");
      retireEntered.resolve();
      assert.equal(oldCanvas.style.visibility, "hidden", "input must not reach stale canvas during retire");
      return retire().then(() => { playing = false; activeAttachment = null; });
    },
    terminate() {
      events.push("terminate"); playing = false; this.terminations += 1;
      this.canvas = { style: {} };
    },
  };
  const client = { terminated: false, run(source, metadata, options) {
    assert.equal(activeAttachment, null, "new source must not mutate the arena before input retirement");
    events.push("author"); authorCalls += 1;
    authorRequests.push({ source, metadata, options });
    return Promise.reject(failure);
  } };
  const context = {
    performance, canvas: oldCanvas, player: started ? oldPlayer : null, playerNeedsRestart: false,
    preparedPlaybackPlayer: started && prepared ? oldPlayer : null,
    sceneRunPromise: null, activeSourceContinuation: null, activeRunRequest: null,
    selectedExampleId: "one", sceneSourceEditor: { value: "same source with a run-dependent failure" },
    status: { dataset: {} }, patchStatus: { dataset: {} }, authoringClient: client,
    currentExample: () => ({ id: "one", title: "One" }),
    generations: { beginRun: () => ({ selectionGeneration: 1, runGeneration: 2 }) },
    isCurrentRun: () => current, recordStale: (_, stage) => ({ stale: true, stage }),
    resetPresentationRate() {}, stopMetricsPolling() {}, updatePreviewRecoveryControl() {},
    beginBusy: () => () => { releases += 1; }, setRuntimeStatus() {},
    authoringSourceDocument, diagnosticPanel: { beginRun() {} },
    previousPreview: { hold() { events.push("hold"); } },
    playbackControls: { setControllable(value) { controllable = value; }, destroy() { controllable = false; } },
    ensureAuthoringClient: () => client,
    ensureRuntimePreparation() { events.push("prepare"); return {}; },
    adoptRuntimeCanvas(owner) { context.canvas = owner.canvas; },
    showSceneError(error) { errors.push(error); }, showError(error) { errors.push(error); },
  };
  if (authoringRunOptions !== undefined) {
    context.__NOON_PLAYGROUND_TEST_HOOKS__ = { authoringRunOptions };
  }
  vm.runInNewContext(production, context);
  return { context, events, errors, failure, oldPlayer, client, authorRequests,
    retireEntered: retireEntered.promise,
    get playing() { return playing; }, get controllable() { return controllable; },
    get authorCalls() { return authorCalls; }, get releases() { return releases; },
    get activeAttachment() { return activeAttachment; }, temporaryAttachment,
    supersede() { current = false; } };
}

test("explicit Run failure stops old playback before authoring behind a frozen preview", async () => {
  const f = fixture();
  const result = await f.context.runScene();
  assert.equal(result.error, f.failure, "Run must reach authoring, not a missing harness dependency");
  assert.equal(result.stale, false);
  assert.equal(f.playing, false, "old replay must not run invisibly behind the frozen error preview");
  assert.equal(f.controllable, false);
  assert.deepEqual(f.events, ["hold", "retire", "author", "hold"]);
  assert.equal(f.context.player, f.oldPlayer, "successful retire keeps the warm execution owner");
  assert.equal(f.oldPlayer.terminations, 0);
  assert.equal(f.client.terminated, false);
  assert.deepEqual(f.errors, [f.failure], "the original authoring failure must stay intact");
  assert.equal(f.context.status.dataset.playbackPlaying, "false");
  assert.equal(f.context.status.dataset.playbackControls, "unavailable");
  assert.equal(f.releases, 1);
});

test("a first Run still initializes normally without pausing an absent owner", async () => {
  const f = fixture({ started: false });
  assert.equal((await f.context.runScene()).error, f.failure);
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
  test(`superseded retire ${reject ? "failure" : "completion"} cannot author old source or retire its successor`, async () => {
    const gate = deferred();
    const f = fixture({ retire: () => gate.promise });
    const run = f.context.runScene();
    // Startup now awaits run options before pausing. Supersede the actual retire,
    // not that earlier boundary, and attach its rejection handler first.
    await f.retireEntered;
    assert.deepEqual(f.events, ["hold", "retire"]);
    assert.equal(f.authorCalls, 0);
    f.supersede();
    const successor = { terminate() { assert.fail("obsolete retire must not retire a newer owner"); } };
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

test("Run waits for the retire acknowledgement and concurrent requests join the same startup", async () => {
  const gate = deferred();
  const f = fixture({ retire: () => gate.promise });
  const first = f.context.runScene();
  const second = f.context.runScene();
  await f.retireEntered;
  assert.deepEqual(f.events, ["hold", "retire"]);
  assert.equal(f.authorCalls, 0);
  assert.equal(f.controllable, false);
  gate.resolve();
  await Promise.all([first, second]);
  assert.equal(f.authorCalls, 1);
  assert.equal(f.releases, 1);
  assert.equal(f.context.sceneRunPromise, null);
});


test("ordinary Run without qualification hooks preserves worker defaults", async () => {
  const f = fixture();
  assert.equal(Object.hasOwn(f.context, "__NOON_PLAYGROUND_TEST_HOOKS__"), false);
  assert.equal((await f.context.runScene()).error, f.failure);
  assert.equal(f.authorRequests.length, 1);
  assert.equal(f.authorRequests[0].options.preparedPlayback, false);
  assert.equal(f.authorRequests[0].options.preparationLookaheadSeconds, 1);
  assert.deepEqual(f.errors, [f.failure]);
});

test("concurrent Runs await one options hook and one retire before authoring", async () => {
  const options = deferred(), retired = deferred();
  const calls = [];
  const f = fixture({ retire: () => retired.promise, authoringRunOptions(payload) {
    calls.push({ ...payload });
    return options.promise;
  } });
  const first = f.context.runScene();
  const second = f.context.runScene();
  assert.deepEqual(calls, [{ exampleId: "one", runGeneration: 2 }]);
  assert.deepEqual(f.events, []);
  assert.equal(f.authorCalls, 0);
  options.resolve({ preparedPlayback: false, preparationLookaheadSeconds: 2 });
  await f.retireEntered;
  assert.deepEqual(f.events, ["hold", "retire"]);
  assert.equal(f.authorCalls, 0);
  retired.resolve();
  const results = await Promise.all([first, second]);
  for (const result of results) assert.equal(result.error, f.failure);
  assert.equal(f.authorCalls, 1);
  assert.equal(f.authorRequests[0].options.preparationLookaheadSeconds, 2);
  assert.equal(f.releases, 1);
  assert.equal(f.context.sceneRunPromise, null);
  assert.deepEqual(f.errors, [f.failure]);
});

for (const reject of [false, true]) {
  test(`superseded options ${reject ? "failure" : "completion"} cannot touch a replacement`, async () => {
    const gate = deferred();
    const f = fixture({ authoringRunOptions: () => gate.promise });
    const run = f.context.runScene();
    assert.deepEqual(f.events, []);
    f.supersede();
    const successor = {
      retireExecutionSource() { f.events.push("successor-retire"); return Promise.resolve(); },
      terminate() { f.events.push("successor-terminate"); },
    };
    f.context.player = successor;
    if (reject) gate.reject(new Error("retired options"));
    else gate.resolve({ preparedPlayback: true });
    assert.equal((await run).stale, true);
    assert.deepEqual(f.events, [], "stale options must not hold, retire, retire or author");
    assert.equal(f.authorCalls, 0);
    assert.equal(f.oldPlayer.terminations, 0);
    assert.equal(f.context.player, successor);
    assert.deepEqual(f.errors, []);
    assert.equal(f.releases, 1);
    assert.equal(f.context.sceneRunPromise, null);
  });
}

for (const prepared of [false, true]) {
  test(`${prepared ? "prepared reader" : "ordinary reader entering prepared execution"} is retired before new authoring`, async () => {
    const f = fixture({ prepared, authoringRunOptions: () => ({ preparedPlayback: true }) });
    assert.equal((await f.context.runScene()).error, f.failure);
    assert.deepEqual(f.events, ["hold", "retire", "terminate", "prepare", "author", "hold"]);
    assert.equal(f.oldPlayer.terminations, 1);
    assert.equal(f.playing, false);
    assert.equal(f.context.player, null);
    assert.equal(f.context.preparedPlaybackPlayer, null);
    assert.equal(f.client.terminated, false);
    assert.equal(f.authorRequests[0].options.preparedPlayback, true);
    assert.deepEqual(f.errors, [f.failure]);
    assert.equal(f.releases, 1);
  });
}

test("prepared reader is retired even when the new Run requests ordinary execution", async () => {
  const f = fixture({ prepared: true });
  assert.equal((await f.context.runScene()).error, f.failure);
  assert.deepEqual(f.events, ["hold", "retire", "terminate", "prepare", "author", "hold"]);
  assert.equal(f.oldPlayer.terminations, 1);
  assert.equal(f.context.preparedPlaybackPlayer, null);
  assert.equal(f.authorRequests[0].options.preparedPlayback, false);
  assert.deepEqual(f.errors, [f.failure]);
});

for (const prepared of [false, true]) {
  test(`${prepared ? "prepared" : "ordinary"} retirement failure prevents prepared replacement and retains its exact owner`, async () => {
    const rejection = new Error("source cleanup rejected");
    const f = fixture({ prepared, retire: () => Promise.reject(rejection),
      authoringRunOptions: () => ({ preparedPlayback: true }) });
    const result = await f.context.runScene();
    assert.equal(result.error, rejection);
    assert.deepEqual(f.events, ["hold", "retire", "hold"]);
    assert.equal(f.authorCalls, 0);
    assert.equal(f.oldPlayer.terminations, 0);
    assert.equal(f.context.player, f.oldPlayer);
    assert.equal(f.context.preparedPlaybackPlayer, prepared ? f.oldPlayer : null);
    assert.equal(f.activeAttachment, prepared ? null : f.temporaryAttachment);
    assert.deepEqual(f.errors, [rejection]);
  });
}
