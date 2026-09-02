import assert from "node:assert/strict";
import test from "node:test";

import { SceneIdentityMap } from "./scene-identity.js";

function canonicalScene(keys) {
  const textId = keys.length;
  return {
    sceneSpec: {
      version: 1,
      objects: [
        ...keys.map((key, id) => ({ id, content: { kind: "geometry", key } })),
        { id: textId, content: { kind: "text" } },
      ],
      tracks: [
        ...keys.map((key, id) => ({ id, object: id, key })),
        { id: textId, object: textId },
      ],
      family_animations: [
        {
          target: 100,
          bindings: [{ semantic_leaf: 101, object: textId }],
          spec: {},
        },
      ],
      camera_object: keys.length === 0 ? null : 0,
    },
    identities: {
      objects: keys.map((key, id) => ({ id, key: `object:${key}` })),
      tracks: keys.map((key, id) => ({ id, key: `track:${key}` })),
    },
  };
}

function assertUnique(values) {
  assert.equal(new Set(values).size, values.length);
}

test("canonical mixed identities stay unique across repeated topology edits", () => {
  const identities = new SceneIdentityMap();
  const sequence = [
    { keys: ["a", "b"], stable: [0, 1] },
    { keys: ["b"], stable: [1] },
    { keys: ["c", "b"], stable: [2, 1] },
    { keys: ["b", "d", "c"], stable: [1, 3, 2] },
    { keys: ["a", "b"], stable: [0, 1] },
  ];

  const results = [];
  for (const { keys, stable } of sequence) {
    const { sceneSpec, identities: authoredIdentities } = canonicalScene(keys);
    const result = identities.stabilizeSceneSpec(sceneSpec, authoredIdentities);
    results.push(result);

    assert.deepEqual(result.objects.slice(0, keys.length).map(({ id }) => id), stable);
    assert.deepEqual(result.tracks.slice(0, keys.length).map(({ id }) => id), stable);
    assertUnique(result.objects.map(({ id }) => id));
    assertUnique(result.tracks.map(({ id }) => id));

    const textObject = result.objects.at(-1).id;
    const textTrack = result.tracks.at(-1).id;
    assert.equal(result.tracks.at(-1).object, textObject);
    assert.equal(result.family_animations[0].bindings[0].object, textObject);
    assert.ok(!stable.includes(textObject));
    assert.ok(!stable.includes(textTrack));
  }

  // c and d own stable IDs 2 and 3 even while absent from the final scene. The
  // canonical-only Text source ID is 2 in that final rerun and must not steal c's
  // historical semantic identity.
  assert.ok(![0, 1, 2, 3].includes(results.at(-1).objects.at(-1).id));
  assert.ok(![0, 1, 2, 3].includes(results.at(-1).tracks.at(-1).id));
});
