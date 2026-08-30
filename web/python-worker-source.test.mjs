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
