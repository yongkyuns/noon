import assert from "node:assert/strict";
import test from "node:test";

import {
  AUTHORING_CHANNEL,
  AUTHORING_PROTOCOL_VERSION,
  PythonAuthoringClient,
  parseAuthoringResult,
  validateCallbackSession,
  validatePatchBatch,
  validateRetainedAuthoringDocument,
  validateSceneDocument,
  validateSceneDuration,
  validateSceneIdentities,
} from "./authoring-client.js";

class FakeWorker {
  listeners = new Map();
  messages = [];
  terminated = false;

  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  postMessage(message) {
    this.messages.push(message);
  }

  terminate() {
    this.terminated = true;
  }

  emit(type, payload) {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(type === "message" ? { data: payload } : payload);
    }
  }
}

function workerMessage(type, payload = {}) {
  return {
    channel: AUTHORING_CHANNEL,
    protocolVersion: AUTHORING_PROTOCOL_VERSION,
    type,
    ...payload,
  };
}

function retainedDocument() {
  return {
    channel: "noon.authoring.retained",
    protocol_version: 1,
    objects: [
      {
        object: 2 ** 52,
        order: 1,
        text: {
          source: "*Hello* from _Typst!_",
          math: false,
          font_size: 96,
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

test("correlates a Python request with a validated PatchBatch response", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const resultPromise = client.run("result = batch", { sequence: 4 });
  await Promise.resolve();
  assert.deepEqual(worker.messages[0], {
    channel: AUTHORING_CHANNEL,
    protocolVersion: AUTHORING_PROTOCOL_VERSION,
    type: "run",
    requestId: 0,
    source: "result = batch",
    context: { sequence: 4 },
  });

  const batch = { version: 1, sequence: 4, patches: [] };
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify({ kind: "patch_batch", document: batch }),
    }),
  );
  assert.deepEqual(await resultPromise, { kind: "patch_batch", document: batch });
});

test("correlates a Python request with Scene callback, retained, and duration metadata", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const resultPromise = client.run("result = scene");
  await Promise.resolve();
  const scene = { version: 1, objects: [{ id: 0 }], tracks: [] };
  const identities = { objects: [{ id: 0, key: "@object:0" }], tracks: [] };
  const callbacks = { session_id: 3, slots: [{ id: 0, objects: [0] }] };
  const retained = retainedDocument();
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify({
        kind: "scene_document",
        document: scene,
        retained_document: retained,
        duration: 2.75,
        identities,
        callbacks,
      }),
    }),
  );

  assert.deepEqual(await resultPromise, {
    kind: "scene_document",
    document: scene,
    retainedDocument: retained,
    duration: 2.75,
    identities,
    callbacks,
  });
});

test("older scene results without a retained sidecar remain compatible", () => {
  const scene = { version: 1, objects: [], tracks: [] };
  const parsed = parseAuthoringResult(
    JSON.stringify({
      kind: "scene_document",
      document: scene,
      duration: 0,
      identities: { objects: [], tracks: [] },
      callbacks: null,
    }),
  );
  assert.equal(parsed.retainedDocument, null);
});

test("validates retained Typst authoring documents and JS-safe identities", () => {
  const retained = retainedDocument();
  assert.equal(validateRetainedAuthoringDocument(retained), retained);

  const unsafe = structuredClone(retained);
  unsafe.objects[0].object = Number.MAX_SAFE_INTEGER + 1;
  assert.throws(
    () => validateRetainedAuthoringDocument(unsafe),
    /invalid object ID/,
  );

  const duplicateOrder = structuredClone(retained);
  duplicateOrder.objects.push({
    ...structuredClone(duplicateOrder.objects[0]),
    object: 2 ** 52 + 1,
  });
  assert.throws(
    () => validateRetainedAuthoringDocument(duplicateOrder),
    /duplicate painter orders/,
  );
});

test("Scene duration accepts zero and rejects missing, negative, or non-finite values", () => {
  assert.equal(validateSceneDuration(0), 0);
  assert.equal(validateSceneDuration(3.5), 3.5);
  assert.throws(() => validateSceneDuration(undefined), /finite and non-negative/);
  assert.throws(() => validateSceneDuration(-0.01), /finite and non-negative/);
  assert.throws(() => validateSceneDuration(Number.NaN), /finite and non-negative/);
  assert.throws(() => validateSceneDuration(Number.POSITIVE_INFINITY), /finite and non-negative/);

  const sceneResult = {
    kind: "scene_document",
    document: { version: 1, objects: [], tracks: [] },
    identities: { objects: [], tracks: [] },
    callbacks: null,
  };
  assert.throws(
    () => parseAuthoringResult(JSON.stringify(sceneResult)),
    /duration must be finite and non-negative/,
  );
});

test("runs one callback phase and validates its PatchBatch", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const frame = {
    time: 0.25,
    delta_time: 0.25,
    objects: [],
    invocations: [{ callback: 0, object_indices: [] }],
  };
  const resultPromise = client.runCallbackPhase(2, frame, 7);
  await Promise.resolve();
  assert.deepEqual(worker.messages[0], {
    channel: AUTHORING_CHANNEL,
    protocolVersion: AUTHORING_PROTOCOL_VERSION,
    type: "callback_phase",
    requestId: 0,
    sessionId: 2,
    frame,
    sequence: 7,
  });

  const batch = { version: 1, sequence: 7, patches: [] };
  worker.emit(
    "message",
    workerMessage("callback_result", {
      requestId: 0,
      patchBatchJson: JSON.stringify(batch),
    }),
  );
  assert.deepEqual(await resultPromise, batch);
});

test("rejects only the request associated with a Python execution error", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const resultPromise = client.run("raise RuntimeError('broken')");
  await Promise.resolve();
  worker.emit(
    "message",
    workerMessage("error", {
      requestId: 0,
      message: "broken",
    }),
  );

  await assert.rejects(resultPromise, /broken/);
});

test("rejects malformed PatchBatch documents before they reach Rust", () => {
  assert.throws(
    () => validatePatchBatch({ version: 99, sequence: 0, patches: [] }),
    /Unsupported Noon IR version 99/,
  );
  assert.throws(
    () => validatePatchBatch({ version: 1, sequence: -1, patches: [] }),
    /non-negative safe integer/,
  );
  assert.throws(
    () => validatePatchBatch({ version: 1, sequence: 0, patches: {} }),
    /must be an array/,
  );
});

test("rejects malformed encoded worker results", () => {
  assert.throws(() => parseAuthoringResult({}), /must be encoded JSON/);
  assert.throws(() => parseAuthoringResult("{"), /returned invalid JSON/);
  assert.throws(
    () => parseAuthoringResult(JSON.stringify({ kind: "unknown" })),
    /Unknown Python authoring result kind/,
  );
});

test("rejects malformed Scene documents before they reach Rust", () => {
  assert.throws(
    () => validateSceneDocument({ version: 1, objects: {}, tracks: [] }),
    /objects must be an array/,
  );
  assert.throws(
    () => validateSceneDocument({ version: 1, objects: [], tracks: {} }),
    /tracks must be an array/,
  );
});

test("rejects Scene identities that do not cover the document", () => {
  assert.throws(
    () =>
      validateSceneIdentities(
        { objects: [], tracks: [] },
        { version: 1, objects: [{ id: 0 }], tracks: [] },
      ),
    /must match its definitions/,
  );
});

test("rejects callback slots that reference objects outside the scene", () => {
  assert.throws(
    () =>
      validateCallbackSession(
        { session_id: 0, slots: [{ id: 0, objects: [4] }] },
        { version: 1, objects: [{ id: 0 }], tracks: [] },
      ),
    /references an invalid object/,
  );
});

test("terminating the client rejects pending work", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const resultPromise = client.run("result = batch");
  await Promise.resolve();
  client.terminate();

  assert.equal(worker.terminated, true);
  await assert.rejects(resultPromise, /terminated/);
});
