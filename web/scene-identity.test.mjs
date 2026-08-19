import assert from "node:assert/strict";
import test from "node:test";

import { diffSceneDocuments, SceneIdentityMap } from "./scene-identity.js";

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

test("diffs compatible documents into semantic patches", () => {
  const current = {
    objects: [{ id: 4, geometry: { circle: { radius: 1 } }, transform: {}, style: { opacity: 1 } }],
    tracks: [],
  };
  const desired = {
    objects: [{ id: 4, geometry: { circle: { radius: 1 } }, transform: {}, style: { opacity: 0.5 } }],
    tracks: [],
  };

  assert.deepEqual(diffSceneDocuments(current, desired), [
    { set_style: { object: 4, style: { opacity: 0.5 } } },
  ]);
});

test("uses replacement fallback for geometry and order changes", () => {
  const first = { id: 0, geometry: { circle: { radius: 1 } } };
  const second = { id: 1, geometry: { circle: { radius: 1 } } };
  assert.equal(
    diffSceneDocuments(
      { objects: [first, second], tracks: [] },
      { objects: [second, first], tracks: [] },
    ),
    null,
  );
  assert.equal(
    diffSceneDocuments(
      { objects: [first], tracks: [] },
      { objects: [{ ...first, geometry: { rectangle: { size: {} } } }], tracks: [] },
    ),
    null,
  );
});

test("compares vector path commands semantically", () => {
  const path = {
    vector_path: {
      commands: [
        { move_to: { to: { x: -1, y: 0 } } },
        { quadratic_to: { control: { x: 0, y: 1 }, to: { x: 1, y: 0 } } },
        "close",
      ],
    },
  };
  const object = { id: 0, geometry: path, transform: {}, style: {} };
  assert.deepEqual(
    diffSceneDocuments(
      { objects: [object], tracks: [] },
      { objects: [structuredClone(object)], tracks: [] },
    ),
    [],
  );

  const changed = structuredClone(object);
  changed.geometry.vector_path.commands[1].quadratic_to.control.y = 2;
  assert.equal(
    diffSceneDocuments(
      { objects: [object], tracks: [] },
      { objects: [changed], tracks: [] },
    ),
    null,
  );
});

test("preserves append-compatible removals and additions", () => {
  const first = { id: 0, geometry: { circle: { radius: 1 } }, transform: {}, style: {} };
  const second = { id: 1, geometry: { circle: { radius: 1 } }, transform: {}, style: {} };
  const appended = { id: 2, geometry: { circle: { radius: 1 } }, transform: {}, style: {} };

  assert.deepEqual(
    diffSceneDocuments(
      { objects: [first, second], tracks: [] },
      { objects: [second, appended], tracks: [] },
    ),
    [{ remove_object: 0 }, { create_object: appended }],
  );
  assert.equal(
    diffSceneDocuments(
      { objects: [first, second], tracks: [] },
      { objects: [first, appended, second], tracks: [] },
    ),
    null,
  );
});

test("compares semantic transforms and tracks without serialization", () => {
  const object = {
    id: 0,
    geometry: { rectangle: { size: { x: 2, y: 3 } } },
    transform: {
      translation: { x: 1, y: 2 },
      rotation: 0,
      scale: { x: 1, y: 1 },
    },
    style: { fill: null, stroke: null, stroke_width: 0, opacity: 1 },
  };
  const track = {
    id: 0,
    object: 0,
    property: "position",
    values: { vec2: { from: { x: 0, y: 0 }, to: { x: 1, y: 1 } } },
    timing: { start_time: 0, duration: 1, easing: "linear" },
  };
  const desiredObject = {
    ...structuredClone(object),
    transform: { ...object.transform, rotation: 0.5 },
  };
  const desiredTrack = {
    ...structuredClone(track),
    timing: { ...track.timing, duration: 2 },
  };

  assert.deepEqual(
    diffSceneDocuments(
      { objects: [object], tracks: [track] },
      { objects: [desiredObject], tracks: [desiredTrack] },
    ),
    [
      { set_transform: { object: 0, transform: desiredObject.transform } },
      { replace_track: desiredTrack },
    ],
  );
});
