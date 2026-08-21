import assert from "node:assert/strict";
import test from "node:test";

import { SceneIdentityMap } from "./scene-identity.js";

test("preserves runtime IDs when Python insertion order changes", () => {
  const identities = new SceneIdentityMap();
  const first = identities.stabilize(
    { version: 1, objects: [{ id: 0 }, { id: 1 }], tracks: [] },
    {
      objects: [
        { id: 0, key: "circle" },
        { id: 1, key: "line" },
      ],
      tracks: [],
    },
  );
  const second = identities.stabilize(
    { version: 1, objects: [{ id: 0 }, { id: 1 }, { id: 2 }], tracks: [] },
    {
      objects: [
        { id: 0, key: "new" },
        { id: 1, key: "circle" },
        { id: 2, key: "line" },
      ],
      tracks: [],
    },
  );

  assert.deepEqual(first.objects.map(({ id }) => id), [0, 1]);
  assert.deepEqual(second.objects.map(({ id }) => id), [2, 0, 1]);
});

test("rewrites track IDs and object references by stable keys", () => {
  const identities = new SceneIdentityMap();
  identities.stabilize(
    {
      version: 1,
      objects: [{ id: 0 }],
      tracks: [{ id: 0, object: 0 }],
    },
    {
      objects: [{ id: 0, key: "hero" }],
      tracks: [{ id: 0, key: "hero.move" }],
    },
  );
  const result = identities.stabilize(
    {
      version: 1,
      objects: [{ id: 0 }, { id: 1 }],
      tracks: [{ id: 0, object: 1 }, { id: 1, object: 0 }],
    },
    {
      objects: [
        { id: 0, key: "other" },
        { id: 1, key: "hero" },
      ],
      tracks: [
        { id: 0, key: "hero.move" },
        { id: 1, key: "other.move" },
      ],
    },
  );

  assert.deepEqual(result.tracks, [
    { id: 0, object: 0 },
    { id: 1, object: 1 },
  ]);
});

function keyedScene(count) {
  return {
    document: {
      version: 1,
      objects: Array.from({ length: count }, (_, id) => ({ id })),
      tracks: [],
    },
    identities: {
      objects: Array.from({ length: count }, (_, id) => ({ id, key: `dot.${id}` })),
      tracks: [],
    },
  };
}

test("grid expansion and shrink preserve surviving semantic IDs", () => {
  const identities = new SceneIdentityMap();
  const initial = keyedScene(180);
  const expanded = keyedScene(200);
  const shrunk = keyedScene(96);

  const first = identities.stabilize(initial.document, initial.identities);
  const second = identities.stabilize(expanded.document, expanded.identities);
  const third = identities.stabilize(shrunk.document, shrunk.identities);

  assert.deepEqual(
    second.objects.slice(0, 180).map(({ id }) => id),
    first.objects.map(({ id }) => id),
  );
  assert.deepEqual(
    third.objects.map(({ id }) => id),
    first.objects.slice(0, 96).map(({ id }) => id),
  );
  assert.deepEqual(second.objects.slice(180).map(({ id }) => id), [180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199]);
});
