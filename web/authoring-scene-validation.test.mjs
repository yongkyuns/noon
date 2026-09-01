import assert from "node:assert/strict";
import test from "node:test";

import { validateSceneDocument } from "./authoring-client.js";

function scene({ objects = [], tracks = [] } = {}) {
  return { version: 1, objects, tracks };
}

test("accepts unique JS-safe legacy Scene definition IDs", () => {
  const document = scene({
    objects: [{ id: 0 }, { id: 2 ** 52 }],
    tracks: [{ id: 1 }, { id: 3 }],
  });

  assert.equal(validateSceneDocument(document), document);
});

test("rejects malformed legacy Scene definition IDs at the authoring boundary", () => {
  for (const document of [
    scene({ objects: [{}] }),
    scene({ objects: [{ id: -1 }] }),
    scene({ objects: [{ id: Number.MAX_SAFE_INTEGER + 1 }] }),
    scene({ tracks: [null] }),
    scene({ tracks: [{ id: -1 }] }),
    scene({ tracks: [{ id: Number.MAX_SAFE_INTEGER + 1 }] }),
  ]) {
    assert.throws(() => validateSceneDocument(document), /invalid (object|track) ID/);
  }
});

test("rejects duplicate legacy Scene object and track IDs", () => {
  assert.throws(
    () => validateSceneDocument(scene({ objects: [{ id: 4 }, { id: 4 }] })),
    /duplicate object IDs/,
  );
  assert.throws(
    () => validateSceneDocument(scene({ tracks: [{ id: 7 }, { id: 7 }] })),
    /duplicate track IDs/,
  );
});
