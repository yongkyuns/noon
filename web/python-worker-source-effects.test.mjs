import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");
const begin = source.indexOf("async function runAuthoringSource("), end = source.indexOf("async function runCanonicalCallbackPhase", begin);
assert.ok(begin >= 0 && end > begin);
function worker() {
  const events = [];
  const run = new Function("withAuthoringDiagnostics", "prepareAuthoringDependencies", "diagnosedHostCode", `
    let activeAuthoringRun = null, authoringLibrariesPristine = true;
    ${source.slice(begin, end)}
    return async (pyodide, preparedPlayback) => {
      activeAuthoringRun = { preparedPlayback, sourceSceneOnly: false };
      await runAuthoringSource(pyodide, "source", {}, 1);
      return activeAuthoringRun.sourceSceneOnly;
    };
  `)(async (_pyodide, _source, _context, _id, execute) => execute({}),
    async () => { events.push("dependencies"); }, value => value);
  const pyodide = proof => ({
    runPython(code) { assert.match(code, /source_is_scene_only\(__noon_source\)/); events.push("proof"); return proof; },
    async runPythonAsync() { events.push("source"); return "result"; },
  });
  return { events, run: (proof, prepared = true) => run(pyodide(proof), prepared) };
}

test("source effects are checked before dependencies or source execute", async () => {
  const h = worker();
  assert.equal(await h.run(true, false), true);
  assert.equal(await h.run(true), true);
  assert.deepEqual(h.events, ["proof", "dependencies", "source", "proof", "dependencies", "source"]);
});

test("an opaque run permanently prevents proof against mutated library state", async () => {
  for (const prepared of [true, false]) {
    const h = worker();
    assert.equal(await h.run(false, prepared), false);
    assert.equal(await h.run(true), false);
    assert.equal(h.events.filter(event => event === "proof").length, 1);
    assert.equal(h.events.filter(event => event === "source").length, 2, "no source retry or reconstruction");
  }
});

test("malformed proof cannot admit source or load its dependencies", async () => {
  const h = worker();
  await assert.rejects(h.run("scene-only"), /proof must be a boolean/);
  assert.deepEqual(h.events, ["proof"]);
});
