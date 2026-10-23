import assert from "node:assert/strict";
import test from "node:test";
import { installStressObservation } from "./playground-stress-observation.mjs";
import { createSourceRestart } from "../web/playground-source-restart.js";

function fixture() {
  const patch = { dataset: { state: "applied", runGeneration: "3" }, value: "Finished" };
  const status = { dataset: { state: "ready", presentedFrames: "110" } };
  const gallery = { runInFlight: false, generationDiagnostics: { runGeneration: 3 } };
  const source = { id: "python-scene-source", value: "rows = 20" };
  const listeners = [];
  const reset = { click: () => {} };
  const elements = new Map([["#patch-status", patch], ["#status", status],
    ["#metric-objects", { value: "626" }], ["#replace-scene", { disabled: false }],
    [".reset-example", reset]]);
  const target = { __noonExampleGallery: gallery,
    document: { body: { scrollHeight: 820 }, querySelector: key => elements.get(key), addEventListener: (name, fn) => {
      assert.equal(name, "input"); listeners.push(fn);
    } } };
  installStressObservation(target);
  return { patch, status, gallery, source, reset, observation: target.__noonStressObservation,
    input: () => listeners.forEach(fn => fn({ target: source })) };
}

test("captures the committed edit before the later automatic build, preserving the restart assertions", async () => {
  const f = fixture(); let timer;
  const restart = createSourceRestart({
    stop: () => { f.gallery.generationDiagnostics.runGeneration += 1; },
    run: () => { f.patch.value = "Building Dynamic Load Stress in the Python worker…"; f.gallery.runInFlight = true; },
    currentSelection: () => "stress", onError: error => { throw error; },
    setTimer: callback => { timer = callback; return 1; }, clearTimer: () => {},
  });
  f.observation.expectSourceEdit("rows = 7");
  f.source.value = "rows = 7";
  // The real target input listener invalidates the old run before the event
  // bubbles to the document observation listener.
  f.patch.value = "Dynamic Load Stress modified · restarting after typing…";
  restart.edited(); f.input();
  timer(); await Promise.resolve(); await Promise.resolve();
  assert.match(f.observation.snapshot().patchText, /Building/);
  f.input(); // a duplicate same-source event must not replace the first capture
  const edited = f.observation.editedSnapshot();
  assert.match(edited.patchText, /restarting/i);
  assert.equal(edited.objectCount, "626");
  assert.equal(edited.runInFlight, false);
  assert.equal(edited.runGeneration, 4);
  assert.equal(f.gallery.runInFlight, true);
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
  f.reset.click = () => { f.patch.value = "Restarting with edited Python…"; f.gallery.generationDiagnostics.runGeneration += 1; };
  const reset = f.observation.snapshot({ reset: true });
  assert.match(reset.patchText, /restarting/i); assert.equal(reset.runGeneration, 4);
});
