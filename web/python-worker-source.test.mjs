import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("./python-worker.js", import.meta.url), "utf8");

test("Python authoring worker keeps request validation helper", () => {
  assert.match(source, /function\s+validateRequest\s*\(/);
  assert.match(source, /function\s+validateHostRequest\s*\(/);
  assert.match(source, /function\s+isRecord\s*\(value\)\s*\{/);
  assert.match(source, /if\s*\(!isRecord\(request\)\s*\|\|\s*request\.channel\s*!==\s*AUTHORING_CHANNEL\)/);
});

test("Python authoring worker keeps shared Arc bridge wiring", () => {
  assert.match(source, /createManimArcSpec/);
  assert.match(source, /createManimArcBetweenPointsSpec/);
  assert.match(source, /WasmManimArcSnapshotQuery/);
  assert.match(source, /noonCreateAuthoringArcSpec/);
  assert.match(source, /noonCreateAuthoringArcBetweenPointsSpec/);
  assert.match(source, /noonQueryAuthoringArc/);
  assert.match(source, /function\s+arcSpecPlain\s*\(/);
  assert.match(source, /function\s+arcSnapshotQueryPlain\s*\(/);
  assert.match(source, /spec\.free\(\)/);
  assert.match(source, /query\.free\(\)/);
});
