import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");
const begin = source.indexOf("async function runAuthoringSource("), end = source.indexOf("async function runCanonicalCallbackPhase", begin);
const timingBegin = source.indexOf("function preparationTimingEvent(");
const timingEnd = source.indexOf("function sourceExecutionRequirement(", timingBegin);
assert.ok(begin >= 0 && end > begin);
assert.ok(timingBegin >= 0 && timingEnd > timingBegin);
function worker() {
  const events = [], diagnostics = [], stores = [];
  class WasmAuthoringStore {
    constructor() { stores.push(this); }
    free() { this.freed = true; }
  }
  const run = new Function("WasmAuthoringStore", "withAuthoringDiagnostics", "prepareAuthoringDependencies", "diagnosedHostCode", "isRecord", "post", `
    let activeAuthoringRun = null, authoringLibrariesPristine = true, fatalAuthoringFailure = false;
    let authoringStore = new WasmAuthoringStore();
    ${source.slice(timingBegin, timingEnd)}
    ${source.slice(begin, end)}
    return async (pyodide, preparedPlayback, diagnosticPreparationTiming = false) => {
      activeAuthoringRun = { requestId: 1, preparedPlayback, sourceSceneOnly: false,
        diagnosticPreparationTiming, preparationTimingOrdinal: 0,
        sourceTurnStartedAtMs: null, continuation: null, completedPreparedContextId: null };
      await runAuthoringSource(pyodide, "source", {}, 1);
      return activeAuthoringRun.sourceSceneOnly;
    };
  `)(WasmAuthoringStore, async (_pyodide, _source, _context, _id, execute) => execute(new Map()),
    async () => { events.push("dependencies"); }, value => value,
    value => value !== null && typeof value === "object" && !Array.isArray(value),
    (type, value) => diagnostics.push({ type, ...value }));
  const pyodide = proof => ({
    runPython(code) {
      assert.match(code, /source_execution_proof\(__noon_source\)/); events.push("proof");
      return JSON.stringify({ admitted: proof, reason: proof ? null : { code: "unknown_name", span: null } });
    },
    async runPythonAsync() { events.push("source"); return "result"; },
  });
  return { events, diagnostics, stores,
    run: (proof, prepared = true, diagnosticPreparationTiming = false) =>
      run(pyodide(proof), prepared, diagnosticPreparationTiming) };
}

test("source effects are checked before dependencies or source execute", async () => {
  const h = worker();
  assert.equal(await h.run(true, false), true);
  assert.equal(await h.run(true), true);
  assert.deepEqual(h.events, ["proof", "dependencies", "source", "proof", "dependencies", "source"]);
  assert.equal(h.stores.length, 3, "each admitted Run has an independent factory arena");
  assert.deepEqual(h.stores.map(store => store.freed === true), [true, true, false]);
});

test("an opaque run permanently prevents proof against mutated library state", async () => {
  for (const prepared of [true, false]) {
    const h = worker();
    assert.equal(await h.run(false, prepared), false);
    assert.equal(await h.run(true), false);
    assert.equal(h.events.filter(event => event === "proof").length, 1);
    assert.equal(h.events.filter(event => event === "source").length, 2, "no source retry or reconstruction");
    assert.equal(h.stores.length, 1, "opaque Runs retain the reusable arena");
    assert.notEqual(h.stores[0].freed, true);
    assert.deepEqual(h.diagnostics.map(event => event.libraryTrust), ["tainted", "tainted"]);
    assert.deepEqual(h.diagnostics.map(event => event.event.stage), ["source-proof", "interpreter-trust"]);
    assert.equal(h.diagnostics[1].event.reason.code, "interpreter_tainted");
  }
});

test("malformed proof cannot admit source or load its dependencies", async () => {
  const h = worker();
  await assert.rejects(h.run("scene-only"), /proof must contain admission/);
  assert.deepEqual(h.events, ["proof"]);
});

test("opt-in source timings include proof and async dependency preparation before Python turns", async () => {
  const h = worker();
  assert.equal(await h.run(true, true, true), true);
  const spans = h.diagnostics.filter(event => event.event.stage === "preparation-timing").map(event => event.event);
  assert.deepEqual(spans.map(span => span.owner), ["source-proof", "dependency-preparation"]);
  assert.deepEqual(spans.map(span => span.ordinal), [0, 1]);
  for (const span of spans) {
    assert.equal(span.runRequestId, 1);
    assert.equal(span.contextId, null, "proof/dependencies precede semantic context registration");
    assert.equal(span.measurement, "wall-clock");
    assert.ok(Number.isFinite(span.startedAtEpochMs));
    assert.ok(Number.isFinite(span.completedAtEpochMs));
    assert.ok(Number.isFinite(span.durationMs) && span.durationMs >= 0);
  }
  assert.deepEqual(h.events, ["proof", "dependencies", "source"]);

  const disabled = worker();
  await disabled.run(true);
  assert.equal(disabled.diagnostics.some(event => event.event.stage === "preparation-timing"), false);
});
