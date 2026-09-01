import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  parseAuthoringResult,
  validateSceneSpec,
} from "./authoring-client.js";

function retainedDocument() {
  return {
    channel: "noon.authoring.retained",
    protocol_version: 2,
    objects: [
      {
        object: 2 ** 52,
        order: 1,
        text: {
          source: "Canonical Noon",
          backend: {
            kind: "native",
            font_family: "DejaVu Sans Mono",
            line_spacing: -1,
          },
          font_size: 48,
          transform: {
            translation: { x: 0, y: 0 },
            scale: { x: 1, y: 1 },
            rotation: 0,
          },
          color: { red: 1, green: 1, blue: 1, alpha: 1 },
          opacity: 1,
        },
      },
    ],
  };
}

function canonicalSceneSpec() {
  return {
    version: 1,
    objects: [
      { id: 0, content: { kind: "geometry" } },
      { id: 2 ** 52, content: { kind: "text" } },
    ],
    tracks: [],
  };
}

test("retained scene results expose canonical SceneSpec beside the compatibility sidecar", () => {
  const document = { version: 1, objects: [{ id: 0 }], tracks: [] };
  const retained = retainedDocument();
  const sceneSpec = canonicalSceneSpec();
  const parsed = parseAuthoringResult(
    JSON.stringify({
      kind: "scene_document",
      document,
      retained_document: retained,
      scene_spec: sceneSpec,
      duration: 1.5,
      identities: { objects: [{ id: 0, key: "@object:0" }], tracks: [] },
      callbacks: null,
    }),
  );

  assert.deepEqual(parsed.sceneSpec, sceneSpec);
  assert.deepEqual(parsed.retainedDocument, retained);
  assert.deepEqual(parsed.sceneSpec.objects.map(({ id }) => id), [0, 2 ** 52]);
});

test("geometry-only scene results carry canonical SceneSpec beside the empty compatibility sidecar", () => {
  const document = { version: 1, objects: [], tracks: [] };
  const sceneSpec = { version: 1, objects: [], tracks: [] };
  const parsed = parseAuthoringResult(
    JSON.stringify({
      kind: "scene_document",
      document,
      retained_document: {
        channel: "noon.authoring.retained",
        protocol_version: 2,
        objects: [],
      },
      scene_spec: sceneSpec,
      duration: 0,
      identities: { objects: [], tracks: [] },
      callbacks: null,
    }),
  );

  assert.deepEqual(parsed.sceneSpec, sceneSpec);
  assert.equal(parsed.retainedDocument.objects.length, 0);
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

test("Python worker canonicalizes retained output through the Rust WASM bridge", async () => {
  const source = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");
  assert.match(source, /canonicalRetainedSceneSpecJson/);
  assert.match(source, /result\.scene_spec\s*=\s*JSON\.parse/);
  assert.match(
    source,
    /canonicalRetainedSceneSpecJson\(JSON\.stringify\(result\.document\), retainedDocumentJson\)/,
  );
  assert.doesNotMatch(source, /retained_document\.objects\.length/);
});
