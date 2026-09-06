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
  assert.match(source, /await\s+execute_construct\(__noon_result\)/);
  assert.match(source, /continuation\.endpoint\.startContinuation\(continuation\.generation\)/);
  assert.match(source, /continuation\.runRequestId\s*!==\s*request\.continuationRunRequestId/);
  assert.match(source, /noonRequireSemanticContinuationActive/);
  const lane = source.slice(
    source.indexOf("async function handleContinuationControl"),
    source.indexOf("async function handleRequest"),
  );
  assert.doesNotMatch(lane, /runPythonAsync/);
});

test("worker delegates every Scene construct lifecycle to the canonical adapter", () => {
  const authoring = source.slice(
    source.indexOf("async function runAuthoringSource"),
    source.indexOf("async function runCallbackPhase"),
  );
  assert.match(authoring, /await\s+execute_construct\(__noon_result\)/);
  assert.doesNotMatch(authoring, /__noon_result\.(?:setup|construct|tear_down)\(/);
  assert.doesNotMatch(authoring, /_(?:begin|finish)_(?:async|synchronous)_continuation_construct/);
});
