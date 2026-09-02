import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  parseAuthoringResult,
  validateSceneSpec,
} from "./authoring-client.js";

function canonicalSceneSpec() {
  return {
    version: 1,
    objects: [
      { id: 0, content: { kind: "geometry" } },
      { id: 1, content: { kind: "text" } },
    ],
    tracks: [],
  };
}

test("mixed scene results require only canonical SceneSpec at the authoring protocol boundary", () => {
  const document = { version: 1, objects: [{ id: 0 }], tracks: [] };
  const sceneSpec = canonicalSceneSpec();
  const parsed = parseAuthoringResult(
    JSON.stringify({
      kind: "scene_document",
      document,
      scene_spec: sceneSpec,
      duration: 1.5,
      identities: { objects: [{ id: 0, key: "@object:0" }], tracks: [] },
      callbacks: null,
    }),
  );

  assert.deepEqual(parsed.sceneSpec, sceneSpec);
  assert.equal("retainedDocument" in parsed, false);
  assert.deepEqual(parsed.sceneSpec.objects.map(({ id }) => id), [0, 1]);
});

test("geometry-only scene results require only canonical SceneSpec", () => {
  const document = { version: 1, objects: [], tracks: [] };
  const sceneSpec = { version: 1, objects: [], tracks: [] };
  const parsed = parseAuthoringResult(
    JSON.stringify({
      kind: "scene_document",
      document,
      scene_spec: sceneSpec,
      duration: 0,
      identities: { objects: [], tracks: [] },
      callbacks: null,
    }),
  );

  assert.deepEqual(parsed.sceneSpec, sceneSpec);
  assert.equal("retainedDocument" in parsed, false);
});

test("canonical SceneSpec result validation rejects duplicate identity and invalid camera references", () => {
  const duplicate = canonicalSceneSpec();
  duplicate.objects[1].id = 0;
  assert.throws(() => validateSceneSpec(duplicate), /duplicate object IDs/);

  const invalidCamera = canonicalSceneSpec();
  invalidCamera.camera_object = 99;
  assert.throws(() => validateSceneSpec(invalidCamera), /invalid camera object/);

  assert.throws(
    () => validateSceneSpec({ version: 99, objects: [], tracks: [] }),
    /Unsupported canonical SceneSpec version/,
  );
});

test("Scene producer owns canonical SceneSpec finalization through the Rust WASM bridge", async () => {
  const [workerSource, pythonSource] = await Promise.all([
    readFile(new URL("./python-worker.source.js", import.meta.url), "utf8"),
    readFile(new URL("./python/noon.py", import.meta.url), "utf8"),
  ]);

  assert.match(workerSource, /self\.noonCanonicalSceneSpecJson = canonicalRetainedSceneSpecJson/);
  assert.match(workerSource, /__noon_scene_spec = __noon_result\.to_scene_spec\(\)/);
  assert.match(workerSource, /"scene_spec": __noon_scene_spec/);
  assert.doesNotMatch(workerSource, /result\.scene_spec\s*=/);
  assert.doesNotMatch(workerSource, /validateRetainedAuthoringDocumentJson/);

  assert.match(pythonSource, /def to_scene_spec\(self\)/);
  assert.match(pythonSource, /from js import noonCanonicalSceneSpecJson as canonicalize/);
  assert.match(pythonSource, /retained_document = getattr\(self, "retained_document", None\)/);
  assert.match(pythonSource, /canonicalize\(legacy_json, retained_json\)/);
});
