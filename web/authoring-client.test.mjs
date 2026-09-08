import assert from "node:assert/strict";
import test from "node:test";

import {
  AUTHORING_CHANNEL,
  AUTHORING_PROTOCOL_VERSION,
  PythonAuthoringClient,
  parseAuthoringResult,
  validateCallbackSession,
  validateSceneDocument,
  validateSceneDuration,
  validateSceneIdentities,
  validateSemanticExecutionDescriptor,
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

function sceneResult(overrides = {}) {
  return {
    kind: "scene_document",
    document: { version: 1, objects: [], tracks: [] },
    scene_spec: { version: 1, objects: [], tracks: [], camera_object: null },
    duration: 0,
    identities: { objects: [], tracks: [] },
    callbacks: null,
    ...overrides,
  };
}

function semanticResult(contextId = "scene") {
  const { kind, duration } = sceneResult();
  return { kind, duration, semantic_execution: { context_id: contextId } };
}

function parsedSemanticResult(contextId = "scene") {
  const { kind, duration } = semanticResult(contextId);
  return { kind, duration, semanticExecution: { contextId } };
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

test("correlates a Python request with a validated shared Scene response", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const resultPromise = client.run("result = scene", { example: "scene" });
  await Promise.resolve();
  assert.deepEqual(worker.messages[0], {
    channel: AUTHORING_CHANNEL,
    protocolVersion: AUTHORING_PROTOCOL_VERSION,
    type: "run",
    requestId: 0,
    source: "result = scene",
    context: { example: "scene" },
    exportDocument: false,
  });

  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify(semanticResult()),
    }),
  );
  assert.deepEqual(await resultPromise, parsedSemanticResult());
});

test("requests legacy Scene export only through an explicit boolean option", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));

  const resultPromise = client.run("result = scene", {}, { exportDocument: true });
  await Promise.resolve();
  assert.equal(worker.messages[0].type, "run");
  assert.equal(worker.messages[0].exportDocument, true);
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify(sceneResult()),
    }),
  );
  await resultPromise;

  await assert.rejects(
    client.run("result = scene", {}, { exportDocument: "yes" }),
    /exportDocument must be a boolean/,
  );
});

test("correlates a Python request with callback, mixed content, and duration metadata", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const resultPromise = client.run("result = scene");
  await Promise.resolve();
  const scene = { version: 1, objects: [{ id: 0 }], tracks: [] };
  const identities = { objects: [{ id: 0, key: "@object:0" }], tracks: [] };
  const callbacks = { session_id: 3, slots: [{ id: 0, objects: [0] }] };
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

test("semantic execution descriptor bypasses legacy document and SceneSpec validation", () => {
  const result = parseAuthoringResult(
    JSON.stringify({
      kind: "scene_document",
      semantic_execution: { context_id: "semantic-7" },
      duration: 2,
      document: "not a legacy scene",
      scene_spec: { version: -1 },
    }),
  );
  assert.deepEqual(result, {
    kind: "scene_document",
    semanticExecution: { contextId: "semantic-7" },
    duration: 2,
  });
  assert.deepEqual(validateSemanticExecutionDescriptor({ context_id: "semantic-8" }), {
    contextId: "semantic-8",
  });
  assert.deepEqual(validateSemanticExecutionDescriptor({
    context_id: "semantic-9",
    continuation_generation: 4,
  }), {
    contextId: "semantic-9",
    continuationGeneration: 4,
  });
  assert.throws(
    () => validateSemanticExecutionDescriptor({ context_id: "" }),
    /non-empty string/,
  );
});

test("routes an early semantic continuation registration without settling the run", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  const registrations = [];
  let settled = false;
  const result = client.run("async scene", {}, {
    onSemanticContinuation: (registration) => { registrations.push(registration); },
  });
  result.then(() => { settled = true; });
  await Promise.resolve();
  worker.emit("message", workerMessage("semantic_continuation_registered", {
    requestId: 0,
    generation: 7,
    duration: 2,
    semanticExecution: {
      context_id: "semantic-async",
      continuation_generation: 7,
    },
  }));
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(settled, false);
  assert.deepEqual(registrations, [{
    generation: 7,
    duration: 2,
    semanticExecution: {
      contextId: "semantic-async",
      continuationGeneration: 7,
    },
  }]);

  const control = new MessageChannel();
  const render = new MessageChannel();
  const attached = client.attachSemanticExecution(
    registrations[0].semanticExecution.contextId,
    control.port1,
    render.port1,
    {
      transportMode: "transferable",
      sharedSlotCapacity: 1024,
      loopDurationSeconds: 2,
      session: 1,
      continuationGeneration: registrations[0].generation,
      pacing: "external_samples",
    },
  );
  await Promise.resolve();
  const attachment = worker.messages.find(
    (message) => message.type === "attach_semantic_execution",
  );
  assert.equal(attachment.continuationGeneration, 7);
  assert.equal(attachment.continuationRunRequestId, 0);
  assert.equal(attachment.pacing, "external_samples");
  worker.emit("message", workerMessage("semantic_execution_attached", {
    requestId: attachment.requestId,
  }));
  await attached;

  worker.emit("message", workerMessage("result", {
    requestId: 0,
    resultJson: JSON.stringify(sceneResult({
      semantic_execution: {
        context_id: "semantic-async",
        continuation_generation: 7,
      },
      duration: 4,
    })),
  }));
  assert.equal((await result).semanticExecution.continuationGeneration, 7);
  control.port2.close();
  render.port2.close();
});

test("waits for continuation startup before settling an already returned result", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  const startup = deferred();
  let settled = false;
  const result = client.run("async scene", {}, {
    onSemanticContinuation: () => startup.promise,
  });
  void result.then(
    () => { settled = true; },
    () => { settled = true; },
  );
  await Promise.resolve();
  worker.emit("message", workerMessage("semantic_continuation_registered", {
    requestId: 0,
    generation: 17,
    duration: 1,
    semanticExecution: {
      context_id: "semantic-delayed",
      continuation_generation: 17,
    },
  }));
  worker.emit("message", workerMessage("result", {
    requestId: 0,
    resultJson: JSON.stringify(sceneResult({
      semantic_execution: {
        context_id: "semantic-delayed",
        continuation_generation: 17,
      },
      duration: 1,
    })),
  }));
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(settled, false);

  startup.resolve();
  assert.equal((await result).semanticExecution.contextId, "semantic-delayed");
});

test("continuation startup failure rejects a buffered result and cancels its run", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  const startup = deferred();
  const result = client.run("async scene", {}, {
    onSemanticContinuation: () => startup.promise,
  });
  await Promise.resolve();
  worker.emit("message", workerMessage("semantic_continuation_registered", {
    requestId: 0,
    generation: 19,
    duration: 1,
    semanticExecution: {
      context_id: "semantic-failed-startup",
      continuation_generation: 19,
    },
  }));
  worker.emit("message", workerMessage("result", {
    requestId: 0,
    resultJson: JSON.stringify(sceneResult({
      semantic_execution: {
        context_id: "semantic-failed-startup",
        continuation_generation: 19,
      },
      duration: 1,
    })),
  }));

  startup.reject(new Error("startup rejected"));
  await assert.rejects(result, /startup rejected/);
  await new Promise((resolve) => setImmediate(resolve));
  const cancellation = worker.messages.find(
    (message) => message.type === "cancel_semantic_continuation",
  );
  assert.equal(cancellation.contextId, "semantic-failed-startup");
  assert.equal(cancellation.continuationGeneration, 19);
  worker.emit("message", workerMessage("semantic_continuation_cancelled", {
    requestId: cancellation.requestId,
  }));
});

test("cancels the matching suspended continuation when early startup rejects", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  const result = client.run("async scene", {}, {
    onSemanticContinuation: () => { throw new Error("renderer startup failed"); },
  });
  const rejected = assert.rejects(result, /renderer startup failed/);
  await Promise.resolve();
  worker.emit("message", workerMessage("semantic_continuation_registered", {
    requestId: 0,
    generation: 11,
    duration: 1,
    semanticExecution: {
      context_id: "semantic-cancel",
      continuation_generation: 11,
    },
  }));
  await Promise.resolve();
  await Promise.resolve();
  await new Promise((resolve) => setImmediate(resolve));
  const cancellation = worker.messages.find(
    (message) => message.type === "cancel_semantic_continuation",
  );
  assert.equal(cancellation.contextId, "semantic-cancel");
  assert.equal(cancellation.continuationGeneration, 11);
  assert.equal(cancellation.continuationRunRequestId, 0);
  assert.equal(cancellation.reason, "renderer startup failed");
  worker.emit("message", workerMessage("semantic_continuation_cancelled", {
    requestId: cancellation.requestId,
  }));
  worker.emit("message", workerMessage("error", {
    requestId: 0,
    message: "renderer startup failed",
  }));
  await rejected;
});

test("semantic execution attachment transfers distinct control and render ports", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  const control = new MessageChannel();
  const render = new MessageChannel();
  const result = client.attachSemanticExecution(
    "semantic-9",
    control.port1,
    render.port1,
    {
      transportMode: "transferable",
      sharedSlotCapacity: 1024,
      loopDurationSeconds: 3,
      session: 4,
      initiallyPaused: true,
    },
  );
  await Promise.resolve();
  const request = worker.messages[0];
  assert.equal(request.type, "attach_semantic_execution");
  assert.equal(request.contextId, "semantic-9");
  assert.equal(request.controlPort, control.port1);
  assert.equal(request.renderPort, render.port1);
  assert.equal(request.session, 4);
  assert.equal(request.initiallyPaused, true);
  worker.emit(
    "message",
    workerMessage("semantic_execution_attached", { requestId: request.requestId }),
  );
  await result;
  const released = client.releaseSemanticExecution("semantic-9");
  await Promise.resolve();
  assert.equal(worker.messages[1].type, "release_semantic_execution");
  worker.emit(
    "message",
    workerMessage("semantic_execution_released", {
      requestId: worker.messages[1].requestId,
    }),
  );
  await released;
  control.port2.close();
  render.port2.close();
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

  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 1,
      resultJson: JSON.stringify(semanticResult("second")),
    }),
  );
  assert.deepEqual(await second, parsedSemanticResult("second"));

  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify(semanticResult("first")),
    }),
  );
  assert.deepEqual(await first, parsedSemanticResult("first"));
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

  const resultPromise = client.run("result = scene");
  await Promise.resolve();
  const response = workerMessage("result", {
    requestId: 0,
    resultJson: JSON.stringify(semanticResult()),
  });
  worker.emit("message", response);
  assert.deepEqual(await resultPromise, parsedSemanticResult());

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
      resultJson: JSON.stringify(semanticResult()),
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

  const resultPromise = client.run("result = scene");
  await Promise.resolve();
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify(semanticResult()),
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
      resultJson: JSON.stringify(semanticResult()),
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

  const resultPromise = client.run("result = scene");
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

test("treats never-issued future response IDs as fatal protocol corruption", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 7,
      resultJson: JSON.stringify(semanticResult()),
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

test("rejects retired PatchBatch authoring responses", () => {
  assert.throws(
    () => parseAuthoringResult(JSON.stringify({
      kind: "patch_batch",
      document: { version: 1, sequence: 0, patches: [] },
    })),
    /Unknown Python authoring result kind: patch_batch/,
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

  const resultPromise = client.run("result = scene");
  await Promise.resolve();
  client.terminate();

  assert.equal(worker.terminated, true);
  assert.equal(client.terminated, true);
  await assert.rejects(resultPromise, /terminated/);
});
