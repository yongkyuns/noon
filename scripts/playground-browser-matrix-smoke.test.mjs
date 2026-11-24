import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { runInNewContext } from "node:vm";

const source = await readFile(new URL("./playground-browser-matrix-smoke.mjs", import.meta.url), "utf8");
const start = source.indexOf("function appliedSceneMatchesCurrentRun(");
const end = source.indexOf("\nasync function waitForAppliedScene", start);
assert.ok(start >= 0 && end > start, "matrix applied-run predicate must remain extractable");
const predicateSource = source.slice(start, end);

function predicate(state = {}, previousRunGeneration = 4) {
  const { selectedId, patchId, patchState, patchGeneration, activeGeneration } = {
    selectedId: "same", patchId: "same", patchState: "applied", patchGeneration: "5", activeGeneration: 5,
    ...state,
  };
  const patch = { dataset: { exampleId: patchId, state: patchState, runGeneration: patchGeneration } };
  const selected = { dataset: { exampleId: selectedId } };
  return runInNewContext(`${predicateSource}\nappliedSceneMatchesCurrentRun(input);`, {
    document: {
      querySelector(selector) {
        if (selector === "#patch-status") return patch;
        if (selector === ".example-card[aria-selected='true']") return selected;
        return null;
      },
    },
    window: { __noonExampleGallery: { executionDiagnostics: { context: { runGeneration: activeGeneration } } } },
    input: { expectedExampleId: "same", previousRunGeneration },
  });
}

test("matrix rerun wait rejects the old applied patch during new-run retirement", () => {
  const oldPredicate = ({ selectedId, patchState }) => selectedId === "same" && patchState === "applied";
  const stale = { selectedId: "same", patchId: "same", patchState: "applied", patchGeneration: "4", activeGeneration: 5 };
  assert.equal(oldPredicate(stale), true);
  assert.equal(predicate(stale), false);
  assert.equal(predicate({ ...stale, activeGeneration: 4 }), false);
  assert.equal(predicate({ ...stale, patchGeneration: "5" }), true);
});

test("matrix rerun wait requires the selected example and finite matching generations", () => {
  assert.equal(predicate({ selectedId: "other" }), false);
  assert.equal(predicate({ patchId: "other" }), false);
  assert.equal(predicate({ patchGeneration: "not-a-number" }), false);
  assert.equal(predicate({ patchGeneration: "" }), false);
  assert.equal(predicate({ activeGeneration: NaN }), false);
  assert.equal(predicate({ activeGeneration: null }), false);
  assert.equal(predicate({ patchGeneration: undefined }), false);
});

test("matrix rerun wait lets the existing post-wait assertion report patch errors", () => {
  assert.equal(predicate({ patchState: "error", patchGeneration: "invalid", activeGeneration: NaN }), true);
});
