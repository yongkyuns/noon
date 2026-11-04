import assert from "node:assert/strict";
import test from "node:test";
import { installStressObservation } from "./playground-stress-observation.mjs";

function fixture() {
  const patch = { dataset: { state: "applied", runGeneration: "3" }, value: "Finished" };
  const draft = { dataset: { state: "applied" }, value: "Test · current Run applied" };
  const status = { dataset: { state: "ready", presentedFrames: "110" } };
  const gallery = { runInFlight: false, generationDiagnostics: { runGeneration: 3 } };
  const source = { id: "python-scene-source", value: "rows = 20" };
  const listeners = [];
  const reset = { click: () => {} };
  const elements = new Map([["#patch-status", patch], ["#draft-status", draft], ["#status", status],
    ["#metric-objects", { value: "626" }], ["#replace-scene", { disabled: false }],
    [".reset-example", reset]]);
  const target = { __noonExampleGallery: gallery,
    document: { body: { scrollHeight: 820 }, querySelector: key => elements.get(key), addEventListener: (name, fn) => {
      assert.equal(name, "input"); listeners.push(fn);
    } } };
  installStressObservation(target);
  return { patch, draft, status, gallery, source, reset, observation: target.__noonStressObservation,
    input: () => listeners.forEach(fn => fn({ target: source })) };
}

test("captures an edit as draft state without changing the active Run", () => {
  const f = fixture();
  f.observation.expectSourceEdit("rows = 7");
  f.source.value = "rows = 7";
  f.draft.value = "Dynamic Load Stress draft not applied · Run to apply";
  f.draft.dataset.state = "unapplied";
  f.input();
  const edited = f.observation.editedSnapshot();
  assert.equal(edited.patchText, "Finished");
  assert.equal(edited.draftText, "Dynamic Load Stress draft not applied · Run to apply");
  assert.equal(edited.draftState, "unapplied");
  assert.equal(edited.objectCount, "626");
  assert.equal(edited.runInFlight, false);
  assert.equal(edited.runGeneration, 3);
  assert.equal(f.patch.dataset.runGeneration, "3");
});

test("requires the expected committed source input instead of inferring it from current text", () => {
  const f = fixture(); f.observation.expectSourceEdit("rows = 7");
  assert.throws(() => f.observation.editedSnapshot(), /No matching committed source input/);
  f.input();
  assert.throws(() => f.observation.editedSnapshot(), /No matching committed source input/);
  f.source.value = "rows = 7";
  assert.throws(() => f.observation.editedSnapshot(), /No matching committed source input/);
  f.input(); assert.equal(f.observation.editedSnapshot().objectCount, "626");
});

test("a new expected edit cannot reuse a previous edit observation", () => {
  const f = fixture(); f.observation.expectSourceEdit("rows = 7"); f.source.value = "rows = 7"; f.input();
  f.observation.expectSourceEdit("rows = 20");
  assert.throws(() => f.observation.editedSnapshot(), /No matching committed source input/);
});

test("reset and state capture share one synchronous browser action", () => {
  const f = fixture();
  f.reset.click = () => { f.draft.value = "Dynamic Load Stress draft not applied · Run to apply"; };
  const reset = f.observation.snapshot({ reset: true });
  assert.equal(reset.patchText, "Finished"); assert.equal(reset.runGeneration, 3);
});
