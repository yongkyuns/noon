import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const runStatusStart = main.indexOf("function updateDraftStatus(");
const runStatusEnd = main.indexOf("function setRunPatchStatus(", runStatusStart);
const setRunStatusEnd = main.indexOf("async function applyExampleInteraction(", runStatusEnd);
const resetStart = main.indexOf('resetButton.addEventListener("click"');
const resetEnd = main.indexOf('sceneSourceEditor.addEventListener(\n  "focus"', resetStart);
const editorStart = main.indexOf("let sourceComposing = false;");
const editorEnd = main.indexOf('window.addEventListener("popstate"', editorStart);
const requestStart = main.indexOf("function requestSceneRun(");
const requestEnd = main.indexOf("async function selectExample(", requestStart);
assert.ok(runStatusStart >= 0 && runStatusEnd > runStatusStart);
assert.ok(setRunStatusEnd > runStatusEnd);
assert.ok(resetStart >= 0 && resetEnd > resetStart && editorStart >= 0 && editorEnd > editorStart);

function editorHarness({ playState, deferRun, activeSource = "source used by active run" }) {
  const handlers = new Map();
  const timers = new Map();
  const submitted = [];
  let nextTimer = 0;
  const example = { id: "one", title: "One" };
  const context = {
    sceneSourceEditor: {
      value: "initial draft",
      addEventListener(name, callback) { handlers.set(name, callback); },
    },
    resetButton: {
      disabled: false,
      addEventListener(name, callback) { handlers.set(`reset:${name}`, callback); },
    },
    autoRunCheckbox: {
      checked: false,
      addEventListener(name, callback) { handlers.set(`auto-run:${name}`, callback); },
    },
    autoRunTimer: null,
    autoRunEpoch: 0,
    setTimeout(callback, delay) {
      assert.equal(delay, 500);
      timers.set(++nextTimer, callback);
      return nextTimer;
    },
    clearTimeout(id) { timers.delete(id); },
    runRequestRouter: {
      request(isCurrent = () => true) {
        const source = context.sceneSourceEditor.value;
        const start = () => { if (isCurrent()) submitted.push(source); };
        if (deferRun) deferRun(start);
        else start();
      },
    },
    draftStatus: { value: "", dataset: {} },
    patchStatus: { value: "active execution status", dataset: { runGeneration: "17", state: "playing" } },
    activeRunIdentity: {
      source: activeSource,
      exampleId: "one",
      title: "One",
      outcome: "running",
      token: { runGeneration: 17 },
    },
    canonicalSource: "canonical source",
    currentExample: () => example,
    drafts: new Map(),
    updateDraftStatus: null,
    playback: { state: playState, runGeneration: 17 },
    worker: { interpreterGeneration: 4 },
    canvas: { style: { visibility: "visible" } },
    stopCalls: 0,
  };
  vm.runInNewContext(
    main.slice(runStatusStart, runStatusEnd) +
      main.slice(requestStart, requestEnd) +
      main.slice(resetStart, resetEnd) +
      main.slice(editorStart, editorEnd),
    context,
  );
  return {
    context, handlers, submitted, timers,
    expireTimers() {
      for (const [id, fire] of timers) {
        timers.delete(id);
        fire();
      }
    },
  };
}

for (const playState of ["playing", "paused", "buffering", "completed", "error"]) {
  test(`editing and Reset leave the ${playState} run and its status untouched`, () => {
    const { context, handlers, timers, submitted } = editorHarness({ playState });
    const runIdentity = context.activeRunIdentity;
    const playback = { ...context.playback };
    const worker = context.worker;
    const visibility = context.canvas.style.visibility;

    context.sceneSourceEditor.value = "new draft";
    handlers.get("input")({ isComposing: false });
    assert.equal(context.drafts.get("one"), "new draft");
    assert.equal(context.draftStatus.dataset.state, "unapplied");
    assert.match(context.draftStatus.value, /Run to apply/);
    assert.equal(context.patchStatus.value, "active execution status");
    assert.equal(context.patchStatus.dataset.runGeneration, "17");

    handlers.get("reset:click")();
    assert.equal(context.sceneSourceEditor.value, "canonical source");
    assert.equal(context.drafts.has("one"), false);
    assert.equal(context.draftStatus.dataset.state, "unapplied",
      "restoring canonical text stays unapplied when the active source differs");
    assert.equal(context.patchStatus.value, "active execution status");
    assert.equal(context.activeRunIdentity, runIdentity);
    assert.deepEqual(context.playback, playback);
    assert.equal(context.worker, worker);
    assert.equal(context.canvas.style.visibility, visibility);
    assert.equal(timers.size, 0);
    assert.deepEqual(submitted, []);
    assert.equal(context.stopCalls, 0);
  });
}

test("Auto-run waits for IME composition and submits the latest draft once", () => {
  const { context, handlers, expireTimers, timers, submitted } = editorHarness({ playState: "playing" });
  const activeRun = context.activeRunIdentity;
  context.autoRunCheckbox.checked = true;
  context.sceneSourceEditor.value = "first edit";
  handlers.get("input")({ isComposing: false });
  assert.equal(timers.size, 1);
  handlers.get("compositionstart")();
  context.sceneSourceEditor.value = "composing text";
  handlers.get("input")({ isComposing: true });
  assert.equal(timers.size, 0);
  handlers.get("compositionend")();
  assert.equal(context.drafts.get("one"), "composing text");
  context.sceneSourceEditor.value = "final text";
  handlers.get("input")({ isComposing: false });
  assert.equal(timers.size, 1);
  assert.match(context.draftStatus.value, /Auto-run pending/);
  assert.equal(context.activeRunIdentity, activeRun);
  assert.equal(context.patchStatus.value, "active execution status");
  assert.deepEqual(submitted, []);
  expireTimers();
  assert.equal(timers.size, 0);
  assert.deepEqual(submitted, ["final text"]);
});

test("turning off Auto-run or pressing Run cancels the pending timer", () => {
  const { context, handlers, expireTimers, timers, submitted } = editorHarness({ playState: "paused" });
  handlers.get("input")({ isComposing: false });
  context.autoRunCheckbox.checked = true;
  handlers.get("auto-run:change")();
  assert.equal(timers.size, 1);
  context.autoRunCheckbox.checked = false;
  handlers.get("auto-run:change")();
  expireTimers();
  assert.match(context.draftStatus.value, /Run to apply/);
  assert.deepEqual(submitted, []);
  context.autoRunCheckbox.checked = true;
  handlers.get("auto-run:change")();
  assert.equal(timers.size, 1);
  context.requestSceneRun();
  expireTimers();
  assert.deepEqual(submitted, ["initial draft"]);
});

test("an Auto-run queued behind source retirement yields to a newer draft", () => {
  const queued = [];
  const { context, handlers, expireTimers, submitted } = editorHarness({
    playState: "playing",
    deferRun: start => queued.push(start),
  });
  context.autoRunCheckbox.checked = true;
  handlers.get("input")({ isComposing: false });
  expireTimers();
  assert.equal(queued.length, 1);
  context.sceneSourceEditor.value = "newer draft";
  handlers.get("input")({ isComposing: false });
  queued.shift()();
  assert.deepEqual(submitted, []);
  expireTimers();
  queued.shift()();
  assert.deepEqual(submitted, ["newer draft"]);
});

test("late status from the active run cannot overwrite a newer selected draft", () => {
  const statusCode = main.slice(runStatusStart, setRunStatusEnd);
  const activeRun = { runGeneration: 17 };
  const context = {
    draftStatus: { value: "", dataset: {} },
    patchStatus: { value: "active execution error", dataset: { state: "error", runGeneration: "17" } },
    activeRunIdentity: {
      source: "old run source", exampleId: "one", title: "One", outcome: "error", token: activeRun,
    },
    selectedExampleId: "two",
    sceneSourceEditor: { value: "new selection draft" },
    currentExample: () => ({ id: "two", title: "Two" }),
    autoRunTimer: null,
    isCurrentRun: token => token === activeRun,
  };
  vm.runInNewContext(statusCode, context);
  context.setRunPatchStatus(activeRun, "late old completion", "applied");
  assert.equal(context.patchStatus.value, "active execution error");
  assert.equal(context.patchStatus.dataset.state, "error");
  assert.equal(context.patchStatus.dataset.runGeneration, "17");
  assert.equal(context.draftStatus.dataset.exampleId, "two");
  assert.equal(context.draftStatus.dataset.activeExampleId, "one");
  assert.match(context.draftStatus.value, /Two draft not applied/);
});

test("a failed Run before runtime adoption keeps the displayed run identity", () => {
  const runStart = main.indexOf("async function runScene(");
  const runEnd = main.indexOf("const runRequestRouter =", runStart);
  const run = main.slice(runStart, runEnd);
  const firstAdoption = run.indexOf("adoptRuntimeCanvas(attachedPlayer)");
  const finalAdoption = run.indexOf("adoptRuntimeCanvas(player)");
  const identityWrites = [...run.matchAll(/activeRunIdentity = \{/g)].map(match => match.index);

  assert.ok(runStart >= 0 && runEnd > runStart);
  assert.equal(identityWrites.length, 2, "identity changes only when a runtime is adopted");
  assert.ok(identityWrites[0] > firstAdoption);
  assert.ok(identityWrites[1] > finalAdoption);

  const context = {
    draftStatus: { value: "", dataset: {} },
    activeRunIdentity: {
      source: "last accepted source", exampleId: "one", title: "One", outcome: "complete",
      token: { runGeneration: 8 },
    },
    selectedExampleId: "two",
    sceneSourceEditor: { value: "another draft" },
    currentExample: () => ({ id: "two", title: "Two" }),
    autoRunTimer: null,
  };
  vm.runInNewContext(main.slice(runStatusStart, runStatusEnd), context);
  context.updateDraftStatus();
  assert.match(context.draftStatus.value, /Two draft not applied · One remains active/);
  assert.equal(context.activeRunIdentity.exampleId, "one");
});

test("editor input submits only through the existing Run router", () => {
  const editorStart = main.indexOf('sceneSourceEditor.addEventListener("input"');
  const editorEnd = main.indexOf('window.addEventListener("popstate"', editorStart);
  const editor = main.slice(editorStart, editorEnd);
  assert.doesNotMatch(editor, /runScene\(|requestSceneRun\(|retireActiveRunForReplacement\(|invalidateRun\(|terminate\(/);
  assert.match(editor, /drafts\.set\(example\.id, sceneSourceEditor\.value\)/);
  assert.match(main.slice(requestStart, requestEnd), /return runRequestRouter\.request\(\)/);
  assert.match(main.slice(resetStart, resetEnd), /sceneSourceEditor\.value = canonicalSource/);
  assert.match(main, /run: \(source, exampleId, selectionGeneration\) => runScene\(source, exampleId, selectionGeneration\)/);
});

test("gallery draft selection keeps the active preview until explicit Run", () => {
  const selectionStart = main.indexOf("async function selectExample(");
  const selectionEnd = main.indexOf("function refreshGalleryFilters(", selectionStart);
  const selection = main.slice(selectionStart, selectionEnd);
  assert.ok(selectionStart >= 0 && selectionEnd > selectionStart);
  assert.doesNotMatch(selection, /previousPreview\.(?:clear|hold)|stopMetricsPolling|retireActiveRunForReplacement/);
  assert.match(selection, /if \(run\) return requestSceneRun\(\)/);
});
