import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const mainSource = await readFile(new URL("./main.js", import.meta.url), "utf8");

test("playground routes retained Python output through canonical SceneSpec", () => {
  assert.match(mainSource, /authored\.sceneSpec/);
  assert.match(mainSource, /startRetainedCanonical\(sceneSpecJson/);
  assert.match(mainSource, /reconcileScene\(sceneJson, \{\s*sceneSpecJson,/s);
  assert.doesNotMatch(mainSource, /authored\.retainedDocument/);
  assert.doesNotMatch(mainSource, /retainedDocumentJson/);
});