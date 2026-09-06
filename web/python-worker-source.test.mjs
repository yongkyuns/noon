import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");

test("Python authoring worker keeps request validation helper", () => {
  assert.match(source, /function\s+validateRequest\s*\(/);
  assert.match(source, /function\s+validateHostRequest\s*\(/);
  assert.match(source, /function\s+isRecord\s*\(value\)\s*\{/);
  assert.match(source, /if\s*\(!isRecord\(request\)\s*\|\|\s*request\.channel\s*!==\s*AUTHORING_CHANNEL\)/);
});

test("Python authoring worker keeps shared sector constructors", () => {
  assert.match(source, /manimAnnularSectorSnapshotJson/);
  assert.match(source, /manimSectorSnapshotJson/);
  assert.match(source, /manimAnnulusSnapshotJson/);
  assert.match(source, /noonCreateAuthoringAnnularSectorHandle/);
  assert.match(source, /noonCreateAuthoringSectorHandle/);
  assert.match(source, /noonCreateAuthoringAnnulusHandle/);
});

test("semantic continuation control bypasses the blocked interpreter request queue", () => {
  assert.match(source, /if\s*\(isContinuationControl\(event\.data\)\)/);
  assert.match(source, /void\s+handleContinuationControl\(event\.data\)/);
  assert.match(source, /requestQueue\s*=\s*requestQueue\.then\(\(\)\s*=>\s*handleRequest/);
  assert.match(
    source,
    /await\s+execute_construct\(\s*__noon_result,\s*export_document=bool\(__noon_export_document\)\s*\)/,
  );
  assert.match(source, /continuation\.endpoint\.startContinuation\(continuation\.generation\)/);
  assert.match(source, /continuation\.runRequestId\s*!==\s*request\.continuationRunRequestId/);
  assert.match(source, /noonRequireSemanticContinuationActive/);
  const lane = source.slice(
    source.indexOf("async function handleContinuationControl"),
    source.indexOf("async function handleRequest"),
  );
  assert.doesNotMatch(lane, /runPythonAsync/);
});

test("semantic continuation delivers required callback work to its suspended source", () => {
  assert.match(source, /noonSetSemanticContinuationCallbackSession/);
  assert.match(source, /noonCompleteSemanticContinuationCallback/);
  assert.match(source, /noonFailSemanticContinuationCallback/);
  assert.match(source, /continuation\.callbackRequest/);
  assert.match(source, /pending\.resolve\(continuationEvent\("callback",\s*\{ phase \}\)\)/);
  assert.match(source, /callback\.resolve\(patchBatchJson\)/);
  assert.match(source, /callback\.reject\(new Error\(message\)\)/);
  assert.match(source, /continuationOnly\s*\?\s*\(frame\)\s*=>\s*requestContinuationCallback/);
});

test("suspended callback reads stay token-pinned and cannot settle after cancellation", () => {
  assert.match(source, /noonReadSemanticContinuationCallback/);
  assert.match(source, /function\s+readContinuationCallback\s*\(/);
  assert.match(source, /continuationCallbackRequest\(context, tokenJson\)/);
  assert.match(source, /callback\.read !== null/);
  assert.match(source, /continuation\.callbackRead\(tokenJson, request\)/);
  assert.match(source, /if \(callback\.read !== null\) callback\.read\.reject\(failure\)/);
  assert.match(source, /semantic continuation callback cannot complete while a callback read is pending/);
});

test("sparse callback proof keeps scalar and inactive-object reads in its updater", async () => {
  const example = await readFile(
    new URL("./python/examples/ordinary_callback_sparse_reads.py", import.meta.url),
    "utf8",
  );
  assert.match(example, /anchor\.get_center\(\)/);
  assert.match(example, /tracker\.get_value\(\)/);
  assert.match(example, /await self\.wait\(0\.25\)/);
  assert.match(example, /phase_counts\[phase_time\].*== 1/);
});

test("worker delegates every Scene construct lifecycle to the canonical adapter", () => {
  const authoring = source.slice(
    source.indexOf("async function runAuthoringSource"),
    source.indexOf("async function runCallbackPhase"),
  );
  assert.match(
    authoring,
    /await\s+execute_construct\(\s*__noon_result,\s*export_document=bool\(__noon_export_document\)\s*\)/,
  );
  assert.doesNotMatch(authoring, /__noon_result\.(?:setup|construct|tear_down)\(/);
  assert.doesNotMatch(authoring, /_(?:begin|finish)_(?:async|synchronous)_continuation_construct/);
});


test("retired callback sessions release only after the active Python run unwinds", async () => {
  const retirementSource = source.slice(
    source.indexOf("function retireSemanticContext"),
    source.indexOf("async function handleHostRequest"),
  );
  let finishRun;
  const activeRun = new Promise((resolve) => { finishRun = resolve; });
  let releases = 0;
  const contexts = new Map();
  const entry = { released: true, endpoints: new Set(), releaseCallbackSession() { releases += 1; } };
  contexts.set("context", entry);
  const retire = new Function("semanticContexts", "postError", "activeRun", `
    let requestQueue = activeRun;
    ${retirementSource}
    return (token, entry) => { retireSemanticContext(token, entry); return requestQueue; };
  `)(contexts, (id, error) => { throw error; }, activeRun);
  const released = retire("context", entry);
  await Promise.resolve();
  assert.equal(contexts.size, 0);
  assert.equal(releases, 0);
  finishRun();
  await released;
  assert.equal(releases, 1);
});
