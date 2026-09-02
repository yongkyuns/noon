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

function retainedDocument(backend = { kind: "typst", math: false }) {
  return {
    channel: "noon.authoring.retained",
    protocol_version: 2,
    objects: [
      {
        object: 2 ** 52,
        order: 1,
        text: {
          source: backend.kind === "native" ? "Native Noon" : "*Hello* from _Typst!_",
          backend,
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
  const sceneSpec = {
    version: 1,
    objects: [
      { id: 0, content: { kind: "geometry" } },
      { id: 2 ** 52, content: { kind: "text" } },
    ],
    tracks: [],
  };
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify({
        kind: "scene_document",
        document: scene,
        retained_document: retained,
        scene_spec: sceneSpec,
        duration: 2.75,
        identities,
        callbacks,
      }),
    }),
  );

  assert.deepEqual(await resultPromise, {
    kind: "scene_document",
    document: scene,
    sceneSpec,
    retainedDocument: retained,
    duration: 2.75,
    identities,
    callbacks,
  });
});

test("scene results without canonical SceneSpec are rejected at the current protocol boundary", () => {
  const scene = { version: 1, objects: [], tracks: [] };
  assert.throws(
    () =>
      parseAuthoringResult(
        JSON.stringify({
          kind: "scene_document",
          document: scene,
          duration: 0,
          identities: { objects: [], tracks: [] },
          callbacks: null,
        }),
      ),
    /must include canonical SceneSpec/,
  );
});

test("validates retained native/Typst authoring documents and JS-safe identities", () => {
  for (const retained of [
    retainedDocument(),
    retainedDocument({
      kind: "native",
      font_family: "DejaVu Sans Mono",
      line_spacing: -1,
    }),
  ]) {
    assert.equal(validateRetainedAuthoringDocument(retained), retained);
  }

  const unsafe = retainedDocument();
  unsafe.objects[0].object = Number.MAX_SAFE_INTEGER + 1;
  assert.throws(
    () => validateRetainedAuthoringDocument(unsafe),
    /invalid object ID/,
  );

  const duplicateOrder = retainedDocument();
  duplicateOrder.objects.push({
    ...structuredClone(duplicateOrder.objects[0]),
    object: 2 ** 52 + 1,
  });
  assert.throws(
    () => validateRetainedAuthoringDocument(duplicateOrder),
    /duplicate painter orders/,
  );

  const invalidBackend = retainedDocument({ kind: "unknown" });
  assert.throws(
    () => validateRetainedAuthoringDocument(invalidBackend),
    /Unsupported Python retained text backend/,
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
    scene_spec: { version: 1, objects: [], tracks: [] },
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
  assert.equal(client.terminated, false, "ordinary Python errors keep the worker reusable");
});

test("accepts pending Python responses that complete out of order", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const first = client.run("result = first");
  const second = client.run("result = second");
  await Promise.resolve();
  assert.deepEqual(
    worker.messages.map(({ requestId }) => requestId),
    [0, 1],
  );

  const secondBatch = { version: 1, sequence: 2, patches: [] };
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 1,
      resultJson: JSON.stringify({ kind: "patch_batch", document: secondBatch }),
    }),
  );
  assert.deepEqual(await second, { kind: "patch_batch", document: secondBatch });

  const firstBatch = { version: 1, sequence: 1, patches: [] };
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify({ kind: "patch_batch", document: firstBatch }),
    }),
  );
  assert.deepEqual(await first, { kind: "patch_batch", document: firstBatch });
  assert.deepEqual(client.diagnostics, {
    nextRequestId: 2,
    pendingRequests: 0,
    staleResponses: 0,
    terminated: false,
  });
});

test("drops duplicate responses for already-issued requests without killing the worker", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const batch = { version: 1, sequence: 0, patches: [] };
  const resultPromise = client.run("result = batch");
  await Promise.resolve();
  const response = workerMessage("result", {
    requestId: 0,
    resultJson: JSON.stringify({ kind: "patch_batch", document: batch }),
  });
  worker.emit("message", response);
  assert.deepEqual(await resultPromise, { kind: "patch_batch", document: batch });

  worker.emit("message", response);
  assert.equal(client.terminated, false);
  assert.equal(worker.terminated, false);
  assert.equal(client.diagnostics.staleResponses, 1);

  const retry = client.run("result = retry");
  await Promise.resolve();
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 1,
      resultJson: JSON.stringify({ kind: "patch_batch", document: batch }),
    }),
  );
  await retry;
  assert.equal(client.diagnostics.staleResponses, 1);
});

test("drops malformed stale result payloads before parsing them", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const batch = { version: 1, sequence: 0, patches: [] };
  const resultPromise = client.run("result = batch");
  await Promise.resolve();
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify({ kind: "patch_batch", document: batch }),
    }),
  );
  await resultPromise;

  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: "{",
    }),
  );
  assert.equal(client.terminated, false);
  assert.equal(worker.terminated, false);
  assert.equal(client.diagnostics.staleResponses, 1);

  const retry = client.run("result = retry");
  await Promise.resolve();
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 1,
      resultJson: JSON.stringify({ kind: "patch_batch", document: batch }),
    }),
  );
  await retry;
  assert.equal(client.terminated, false);
});

test("malformed pending payloads remain fatal and reject the pending request", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const resultPromise = client.run("result = batch");
  await Promise.resolve();
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: "{",
    }),
  );

  await assert.rejects(resultPromise, /returned invalid JSON/);
  assert.equal(client.terminated, true);
  assert.equal(worker.terminated, true);
  assert.equal(client.diagnostics.pendingRequests, 0);
  assert.equal(client.diagnostics.staleResponses, 0);
});

test("drops malformed stale callback payloads before parsing them", async () => {
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
  const batch = { version: 1, sequence: 7, patches: [] };
  const callbackPromise = client.runCallbackPhase(2, frame, 7);
  await Promise.resolve();
  worker.emit(
    "message",
    workerMessage("callback_result", {
      requestId: 0,
      patchBatchJson: JSON.stringify(batch),
    }),
  );
  await callbackPromise;

  worker.emit(
    "message",
    workerMessage("callback_result", {
      requestId: 0,
      patchBatchJson: "{",
    }),
  );
  assert.equal(client.terminated, false);
  assert.equal(worker.terminated, false);
  assert.equal(client.diagnostics.staleResponses, 1);
});

test("treats never-issued future response IDs as fatal protocol corruption", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 7,
      resultJson: JSON.stringify({
        kind: "patch_batch",
        document: { version: 1, sequence: 0, patches: [] },
      }),
    }),
  );

  assert.equal(client.terminated, true);
  assert.equal(worker.terminated, true);
  assert.equal(client.diagnostics.staleResponses, 0);
  await assert.rejects(client.run("result = retry"), /terminated/);
});

test("exposes fatal Python worker termination to recovery owners", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();
  assert.equal(client.terminated, false);

  const resultPromise = client.run("result = scene");
  await Promise.resolve();
  worker.emit("error", {
    message: "worker crashed",
    error: { stack: "Error: worker crashed\n    at initializePyodide (python-worker.js:42:7)" },
  });

  await assert.rejects(resultPromise, /initializePyodide \(python-worker\.js:42:7\)/);
  assert.equal(client.terminated, true);
  assert.equal(worker.terminated, true);
  await assert.rejects(client.run("result = retry"), /terminated/);
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
  assert.equal(client.terminated, true);
  await assert.rejects(resultPromise, /terminated/);
});
