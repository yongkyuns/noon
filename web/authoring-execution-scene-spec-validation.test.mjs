import assert from "node:assert/strict";
import test from "node:test";

import { AuthoringExecutionClient } from "./authoring-execution-client.js";

class TestCanvas {
  clientWidth = 640;
  clientHeight = 360;
}

globalThis.HTMLCanvasElement = TestCanvas;

function createClient() {
  return new AuthoringExecutionClient(new TestCanvas());
}

function sceneSpec(objects, cameraObject = undefined) {
  const spec = { version: 1, objects, tracks: [] };
  if (cameraObject !== undefined) {
    spec.camera_object = cameraObject;
  }
  return JSON.stringify(spec);
}

test("canonical retained startup rejects invalid object IDs before worker startup", async () => {
  const invalidObjects = [
    {},
    { id: -1 },
    { id: Number.MAX_SAFE_INTEGER + 1 },
    { id: "1" },
  ];

  for (const object of invalidObjects) {
    await assert.rejects(
      createClient().startRetainedCanonical(sceneSpec([object])),
      /invalid object ID/,
    );
  }
});

test("canonical retained startup rejects duplicate object IDs before worker startup", async () => {
  await assert.rejects(
    createClient().startRetainedCanonical(sceneSpec([{ id: 7 }, { id: 7 }])),
    /duplicate object IDs/,
  );
});

test("canonical retained startup rejects invalid camera references before worker startup", async () => {
  for (const cameraObject of [-1, 2, Number.MAX_SAFE_INTEGER + 1, "1"]) {
    await assert.rejects(
      createClient().startRetainedCanonical(sceneSpec([{ id: 1 }], cameraObject)),
      /invalid camera object/,
    );
  }
});
