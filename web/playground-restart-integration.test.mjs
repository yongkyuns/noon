import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import vm from "node:vm";
import { createSourceRestart } from "./playground-source-restart.js";
import { createRunRequestRouter } from "./playground-run-request-router.js";
import { PlaygroundGeneration } from "./playground-generation.js";
import { authoringSourceDocument } from "./authoring-diagnostic-runtime.js";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}
async function flush() {
  for (let i = 0; i < 30; i += 1) await Promise.resolve();
}
function fixture() {
  let source = "original";
  let selection = "one";
  let active = null;
  const cancellation = deferred();
  const starts = [];
  const timers = new Map();
  const errors = [];
  let timerId = 0;
  const run = () => {
    const gate = deferred();
    const request = { token: {}, source };
    active = { gate, request };
    starts.push({ gate, source });
    return gate.promise.finally(() => { if (active?.gate === gate) active = null; });
  };
  const router = createRunRequestRouter({
    currentSource: () => source,
    currentRun: () => active?.gate.promise ?? null,
    activeSourceContinuation: () => active,
    activeRunRequest: () => active?.request,
    isActiveRunCurrent: (token) => token === active?.request.token,
    run,
    supersede: () => cancellation.promise.then(() => true),
    onQueued() {},
  });
  const restart = createSourceRestart({
    stop: () => Promise.all([cancellation.promise, active?.gate.promise]),
    run: (isCurrent) => router.request(isCurrent),
    currentSelection: () => selection,
    onError: (error) => errors.push(error),
    setTimer(callback) { const id = ++timerId; timers.set(id, callback); return id; },
    clearTimer(id) { timers.delete(id); },
  });
  return {
    restart, starts, errors,
    edit() { source = "edited"; restart.edited(); },
    select() { selection = "two"; restart.cancel(); },
    retire() { cancellation.resolve(); active?.gate.resolve(); },
    tick() { const pending = [...timers.values()]; timers.clear(); for (const callback of pending) callback(); },
  };
}

test("an edit during an explicit Run's cancellation cannot bypass the new typing debounce", async () => {
  const f = fixture();
  const initial = f.restart.runNow(); await flush();
  const explicit = f.restart.runNow(); await flush();
  f.edit();
  f.retire(); await flush();
  assert.equal(f.starts.length, 1, "the obsolete explicit request must not restart before the edit timer");
  f.tick(); await flush();
  assert.deepEqual(f.starts.map((entry) => entry.source), ["original", "edited"]);
  f.retire();
  await Promise.all([initial, explicit]);
  assert.deepEqual(f.errors, []);
});

test("selection change and disposal prevent a queued explicit Run from reviving source", async () => {
  for (const dispose of [false, true]) {
    const f = fixture();
    const initial = f.restart.runNow(); await flush();
    const explicit = f.restart.runNow(); await flush();
    if (dispose) f.restart.dispose(); else f.select();
    f.retire(); await flush();
    assert.equal(f.starts.length, 1);
    await Promise.all([initial, explicit]);
    assert.deepEqual(f.errors, []);
  }
});

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const runStart = main.indexOf("async function runScene()");
const runEnd = main.indexOf("const runRequestRouter =", runStart);
const discardStart = main.indexOf("function discardEarlyContinuationRuntime(");
const discardEnd = main.indexOf("async function supersedeActiveSourceContinuation()", discardStart);
const hideStart = main.indexOf('  window.addEventListener(\n    "pagehide"');
const hideEnd = main.indexOf("\n} catch (error) {", hideStart);
assert.ok(runStart >= 0 && runEnd > runStart && hideStart >= 0 && hideEnd > hideStart);

// Execute the actual Run and navigation handlers: worker retirement rejects an
// in-flight authoring request, as it does when Safari caches a gallery document.
function navigationFixture() {
  const errors = [], clients = [], owners = [], timers = new Map();
  let timerId = 0, fullscreenDisposals = 0, diagnosticInvalidations = 0, diagnosticDisposals = 0;
  const diagnosticBegins = [];
  const window = new EventTarget();
  const diagnostics = { hidden: false };
  const context = {
    window, performance, selectedExampleId: "one", sceneSourceEditor: { value: "source" },
    generations: new PlaygroundGeneration(), status: { dataset: {} }, patchStatus: { dataset: {} },
    player: null, playerNeedsRestart: false, playbackControls: null, runtimePreparation: null,
    authoringClient: null,
    sceneRunPromise: null, activeSourceContinuation: null, activeRunRequest: null,
    latestRunPhaseMetrics: null, canvas: { id: "original", style: {} }, playbackDurationSeconds: 1,
    document: { querySelector: () => diagnostics },
    authoringSourceDocument,
    previousPreview: { hold() {}, showLive() {}, accept() {}, clear() {}, dispose() {} },
    diagnosticPanel: {
      beginRun(document, source) { diagnosticBegins.push({ document, source }); },
      invalidate() { diagnosticInvalidations += 1; },
      dispose() { diagnosticDisposals += 1; },
    },
    currentExample: () => ({ id: "one", title: "One" }),
    resetPresentationRate() {}, stopMetricsPolling() {},
    beginBusy: () => () => {}, setBusy() {},
    setRuntimeStatus(value) { context.runtimeStatus = value; },
    showSceneError: error => errors.push(error), showError: error => errors.push(error),
    recordStale: (_, stage) => ({ stale: true, stage }),
    isCurrentRun: token => context.generations.isRunCurrent(token, "one"),
    adoptRuntimeCanvas: owner => { context.canvas = owner.canvas; },
    disposeFullscreen: () => { fullscreenDisposals += 1; },
    ensureAuthoringClient() {
      if (context.authoringClient === null || context.authoringClient === undefined) {
        const gate = deferred();
        const client = {
          terminated: false, run: () => gate.promise,
          terminate() {
            this.terminated = true;
            gate.reject(new Error("Python authoring client was terminated"));
          },
        };
        clients.push(client); context.authoringClient = client;
      }
      return context.authoringClient;
    },
    ensureRuntimePreparation() {
      const candidate = {
        canvas: context.canvas, terminations: 0,
        pause: () => Promise.resolve(),
        retireExecutionSource: () => Promise.resolve(),
        terminate() { this.terminations += 1; this.canvas = { id: `replacement-${owners.length}`, style: {} }; },
      };
      owners.push(candidate);
      context.runtimePreparation = { candidate };
      return context.runtimePreparation;
    },
  };
  context.sourceRestart = createSourceRestart({
    stop: () => context.generations.invalidateRun(), run: () => context.runScene(),
    currentSelection: () => "one", onError: error => errors.push(error),
    setTimer(callback) { const id = ++timerId; timers.set(id, callback); return id; },
    clearTimer(id) { timers.delete(id); },
  });
  vm.runInNewContext(
    main.slice(discardStart, discardEnd) + main.slice(runStart, runEnd) + main.slice(hideStart, hideEnd),
    context,
  );
  return {
    context, errors, clients, owners, timers,
    get fullscreenDisposals() { return fullscreenDisposals; },
    get diagnosticInvalidations() { return diagnosticInvalidations; },
    get diagnosticDisposals() { return diagnosticDisposals; },
    get diagnosticBegins() { return diagnosticBegins; },
    dispatch(type, persisted) {
      const event = new Event(type); Object.defineProperty(event, "persisted", { value: persisted });
      window.dispatchEvent(event);
    },
    tick() { const pending = [...timers.values()]; timers.clear(); for (const callback of pending) callback(); },
  };
}

test("leaving the gallery retires a pending Python run without publishing a Python error", async () => {
  for (const playing of [false, true]) {
    const f = navigationFixture();
    if (playing) {
      f.context.player = f.context.ensureRuntimePreparation().candidate;
      f.context.runtimePreparation = null;
      f.context.activeSourceContinuation = {};
    }
    const run = f.context.runScene();
    await flush(); // Reach pending authoring after the retained replay pause.
    f.dispatch("pagehide", false);
    assert.equal((await run).stale, true);
    assert.deepEqual(f.errors, []);
    assert.equal(f.clients[0].terminated, true);
    assert.equal(f.owners[0].terminations, 1);
    assert.equal(f.context.activeSourceContinuation, null);
    assert.equal(f.context.player, null);
    assert.equal(f.context.canvas, f.owners[0].canvas);
    assert.equal(f.diagnosticInvalidations, 1, "navigation must invalidate obsolete source diagnostics");
    assert.equal(f.diagnosticDisposals, 1, "permanent navigation disposes diagnostic listeners");
  }
});

test("edited Run retires pre-continuation warmup before admitting its replacement", async () => {
  const f = navigationFixture();
  const start = main.indexOf("async function supersedeActiveSourceContinuation()");
  const end = main.indexOf("function sameSemanticContinuation(", start);
  assert.ok(start >= 0 && end > start);
  vm.runInNewContext(main.slice(start, end), f.context);
  const original = f.context.runScene();
  await flush();
  assert.equal(f.clients.length, 1);
  f.context.sceneSourceEditor.value = "edited source";
  let current = true;
  const router = createRunRequestRouter({
    currentSource: () => f.context.sceneSourceEditor.value,
    currentRun: () => f.context.sceneRunPromise,
    activeSourceContinuation: () => f.context.activeSourceContinuation,
    activeRunRequest: () => f.context.activeRunRequest,
    isActiveRunCurrent: token => token !== undefined && f.context.isCurrentRun(token),
    run: () => f.context.runScene(),
    supersede: () => f.context.supersedeActiveSourceContinuation(),
    onQueued() {},
  });
  const replacement = router.request(() => current);
  try {
    await flush();
    assert.equal(f.clients.length, 2, "edited source must not wait for old warmup to publish");
    assert.equal(f.clients[0].terminated, true);
    assert.equal((await original).stale, true);
    assert.equal(f.diagnosticBegins.at(-1).source, "edited source");
    assert.deepEqual(f.errors, []);
  } finally {
    current = false;
    f.dispatch("pagehide", false);
    await replacement;
    await original;
  }
});

test("Safari cached return keeps explicit Run, edits and subsequent navigation teardown functional", async () => {
  const f = navigationFixture();
  for (let cycle = 0; cycle < 3; cycle += 1) {
    let run;
    if (cycle === 0) run = f.context.runScene();
    else if (cycle === 1) { run = f.context.sourceRestart.runNow(); await flush(); }
    else { f.context.sourceRestart.edited(); f.tick(); await flush(); run = f.context.sceneRunPromise; }
    assert.equal(f.clients.length, cycle + 1, "restored controls must start a fresh authoring request");
    f.dispatch("pagehide", true);
    await run;
    assert.equal(f.diagnosticInvalidations, cycle + 1);
    assert.equal(f.diagnosticDisposals, 0,
      "cached documents must retain diagnostic listeners for the next edit");
    assert.equal(f.clients[cycle].terminated, true);
    assert.equal(f.owners[cycle].terminations, 1);
    assert.equal(f.context.canvas, f.owners[cycle].canvas, "restart must use the replacement, untransferred canvas");
    f.dispatch("pageshow", true);
    assert.equal(f.context.status.dataset.playbackPlaying, "false");
    assert.equal(f.context.status.dataset.runtimeStartup, "deferred");
  }
  assert.deepEqual(f.errors, []);
  assert.equal(f.fullscreenDisposals, 0, "cached DOM controls remain installed");
  f.dispatch("pagehide", false);
  assert.equal(f.fullscreenDisposals, 1, "permanent departure disposes DOM controls");
  assert.equal(f.diagnosticInvalidations, 4);
  assert.equal(f.diagnosticDisposals, 1, "permanent departure disposes diagnostic listeners once");
  assert.ok(f.diagnosticBegins.length >= 3, "each restored explicit source run reopens diagnostics");
  assert.ok(f.diagnosticBegins.every(x => x.document.filename.startsWith("noon-s")),
    "diagnostic runs retain actual host-owned source identities");
  await f.context.sourceRestart.runNow();
  assert.equal(f.clients.length, 3, "a permanently retired document must not start another worker");
});

test("navigation cancels a queued edit restart before the cached document returns", async () => {
  const f = navigationFixture();
  f.context.sourceRestart.edited();
  assert.equal(f.timers.size, 1);
  f.dispatch("pagehide", true); f.dispatch("pageshow", true); f.tick(); await flush();
  assert.equal(f.diagnosticInvalidations, 1);
  assert.equal(f.diagnosticDisposals, 0);
  assert.equal(f.clients.length, 0, "restoration must not launch a cancelled edit timer");
  assert.equal(f.timers.size, 0);
  assert.deepEqual(f.errors, []);
});

// Run admission stays available while automatic preloading owns an authoring
// request. The request router joins an unchanged request or queues an edit.
function busyControlsFixture() {
  const cards = [{ disabled: false }];
  const context = {
    sceneButton: { disabled: false }, resetButton: {}, sceneSourceEditor: { value: "source" },
    canonicalSource: "source", gallerySearch: {}, categorySelect: {}, paritySelect: {},
    previousGalleryPage: {}, nextGalleryPage: {}, galleryPage: 0,
    galleryGrid: { querySelectorAll: () => cards },
    playbackControls: { setBusy(value) { context.playbackBusy = value; } },
    activeRunRequest: null, sceneRunPromise: null, activeSourceContinuation: null,
    current: true, isCurrentRun: () => context.current,
    updateGalleryPagerControls() {},
  };
  const start = main.indexOf("function setBusy(busy)");
  const end = main.indexOf("function beginBusy()", start);
  assert.ok(start >= 0 && end > start);
  vm.runInNewContext(main.slice(start, end), context);
  return { context, cards };
}

test("Run accepts a join or queued edit during preloading before a continuation exists", () => {
  const { context, cards } = busyControlsFixture();
  context.activeRunRequest = { token: {} };
  // beginBusy runs after the active request is published, before its promise.
  context.setBusy(true);
  assert.equal(context.sceneButton.disabled, false);
  context.sceneRunPromise = Promise.resolve();
  context.setBusy(true);
  assert.equal(context.sceneButton.disabled, false);
  assert.equal(context.gallerySearch.disabled, true);
  assert.equal(cards[0].disabled, true);
  assert.equal(context.playbackBusy, true);
});

test("Run remains blocked for selection changes and stale authoring requests", () => {
  const { context } = busyControlsFixture();
  context.setBusy(true);
  assert.equal(context.sceneButton.disabled, true);
  context.activeRunRequest = { token: {} };
  context.sceneRunPromise = Promise.resolve();
  context.current = false;
  context.setBusy(true);
  assert.equal(context.sceneButton.disabled, true);
  context.setBusy(false);
  assert.equal(context.sceneButton.disabled, false);
});
