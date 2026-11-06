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
assert.ok(runStatusStart >= 0 && runStatusEnd > runStatusStart);
assert.ok(setRunStatusEnd > runStatusEnd);
assert.ok(resetStart >= 0 && resetEnd > resetStart && editorStart >= 0 && editorEnd > editorStart);

function editorHarness({ playState, activeSource = "source used by active run" }) {
  const handlers = new Map();
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
    runCalls: 0,
    stopCalls: 0,
  };
  vm.runInNewContext(
    main.slice(runStatusStart, runStatusEnd) +
      main.slice(resetStart, resetEnd) +
      main.slice(editorStart, editorEnd),
    context,
  );
  return { context, handlers };
}

for (const playState of ["playing", "paused", "buffering", "completed", "error"]) {
  test(`editing and Reset leave the ${playState} run and its status untouched`, () => {
    const { context, handlers } = editorHarness({ playState });
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
    assert.equal(context.runCalls, 0);
    assert.equal(context.stopCalls, 0);
  });
}

test("IME composition updates draft status without replacing the active source", () => {
  const { context, handlers } = editorHarness({ playState: "playing" });
  const activeRun = context.activeRunIdentity;
  handlers.get("compositionstart")();
  context.sceneSourceEditor.value = "composing text";
  handlers.get("input")({ isComposing: true });
  handlers.get("compositionend")();
  assert.equal(context.drafts.get("one"), "composing text");
  assert.equal(context.draftStatus.dataset.state, "unapplied");
  assert.equal(context.activeRunIdentity, activeRun);
  assert.equal(context.patchStatus.value, "active execution status");
  assert.equal(context.runCalls, 0);
  assert.equal(context.stopCalls, 0);
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
  };
  vm.runInNewContext(main.slice(runStatusStart, runStatusEnd), context);
  context.updateDraftStatus();
  assert.match(context.draftStatus.value, /Two draft not applied · One remains active/);
  assert.equal(context.activeRunIdentity.exampleId, "one");
});

test("editor lifecycle has no automatic source replacement path", () => {
  const editorStart = main.indexOf('sceneSourceEditor.addEventListener("input"');
  const editorEnd = main.indexOf('window.addEventListener("popstate"', editorStart);
  const editor = main.slice(editorStart, editorEnd);
  assert.doesNotMatch(main, /sourceRestart|createSourceRestart/);
  assert.doesNotMatch(editor, /runScene\(|requestSceneRun\(|retireActiveRunForReplacement\(|invalidateRun\(|terminate\(/);
  assert.match(editor, /drafts\.set\(example\.id, sceneSourceEditor\.value\)/);
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
