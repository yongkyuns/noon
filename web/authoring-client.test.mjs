import assert from "node:assert/strict";
import test from "node:test";
import { runtimeBuildIdentity } from "./test-support/runtime-build.mjs";

import {
  AUTHORING_CHANNEL,
  AUTHORING_PROTOCOL_VERSION,
  PythonAuthoringClient,
  parseAuthoringResult,
  validateSceneDuration,
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
    ...(type === "ready" ? { buildIdentity: runtimeBuildIdentity() } : {}),
    ...payload,
  };
}

function sceneResult(overrides = {}) {
  return { kind: "semantic_scene", semantic_execution: { context_id: "scene" }, duration: 0, ...overrides };
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

test("prepared source options validate before posting and require a continuation consumer", async () => {
  const worker = new FakeWorker(), client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready")); await client.ready();
  for (const options of [
    { preparedPlayback: "yes" },
    ...[0, -1, NaN, Infinity].map(preparationLookaheadSeconds => ({ preparationLookaheadSeconds })),
    { preparedPlayback: true },
  ]) {
    await assert.rejects(client.run("class Demo(Scene): pass", {}, options),
      /boolean|positive and finite|continuation consumer/);
  }
  assert.equal(worker.messages.length, 0); client.terminate();
});

test("prepared continuation metadata permits initial pause and prevents source reattachment", async () => {
  const worker = new FakeWorker(), client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready")); await client.ready();
  let registration;
  const result = client.run("prepared source", {}, { preparedPlayback: true,
    preparationLookaheadSeconds: 0.75, onSemanticContinuation(value) { registration = value; } });
  await Promise.resolve();
  assert.equal(worker.messages[0].preparedPlayback, true);
  assert.equal(worker.messages[0].preparationLookaheadSeconds, 0.75);
  const descriptor = { context_id: "prepared", continuation_generation: 3, prepared_playback: true };
  worker.emit("message", workerMessage("semantic_continuation_registered", {
    requestId: 0, generation: 3, duration: 1, semanticExecution: descriptor,
  }));
  await Promise.resolve();
  assert.equal(registration.semanticExecution.preparedPlayback, true);
  const control = new MessageChannel(), render = new MessageChannel();
  const options = { transportMode: "transferable", sharedSlotCapacity: 1024,
    loopDurationSeconds: 1, session: 7, continuationGeneration: 3, initiallyPaused: true };
  try {
    const attached = client.attachSemanticExecution("prepared", control.port1, render.port1, options);
    await Promise.resolve();
    const request = worker.messages.at(-1);
    assert.equal(request.initiallyPaused, true);
    assert.equal(request.continuationRunRequestId, 0);
    assert.equal("preparedPlayback" in request, false, "source registration owns preparation mode");
    worker.emit("message", workerMessage("semantic_execution_attached", { requestId: request.requestId }));
    await attached;
    for (const override of [{ replaceExistingEndpoint: true }, { continuationGeneration: null },
      { callbackSessionId: 2 }, { pacing: "external_samples" }]) {
      await assert.rejects(client.attachSemanticExecution("prepared", control.port1, render.port1,
        { ...options, ...override }), /reattach|replace|without Python callbacks/);
    }
    assert.equal(worker.messages.length, 2);
    worker.emit("message", workerMessage("result", { requestId: 0,
      resultJson: JSON.stringify({ kind: "semantic_scene", semantic_execution: descriptor, duration: 2 }) }));
    assert.equal((await result).semanticExecution.preparedPlayback, true);
  } finally {
    control.port1.close(); control.port2.close(); render.port1.close(); render.port2.close(); client.terminate();
  }
});

test("preparation metadata cannot cross a different source run mode", async () => {
  for (const preparedPlayback of [false, true]) {
    const worker = new FakeWorker(), client = new PythonAuthoringClient(worker);
    worker.emit("message", workerMessage("ready")); await client.ready();
    const result = client.run("source", {}, { preparedPlayback, onSemanticContinuation() {} });
    await Promise.resolve();
    worker.emit("message", workerMessage("semantic_continuation_registered", {
      requestId: 0, generation: 1, duration: 1,
      semanticExecution: { context_id: "context", continuation_generation: 1, prepared_playback: !preparedPlayback },
    }));
    await assert.rejects(result, /preparation mode/);
    assert.equal(client.terminated, true);
  }
});

test("prepared descriptors preserve optional continuation identity and reject required callbacks", () => {
  const descriptor = { context_id: "context", continuation_generation: 1, prepared_playback: true };
  assert.deepEqual(validateSemanticExecutionDescriptor(descriptor), {
    contextId: "context", continuationGeneration: 1, preparedPlayback: true,
  });
  for (const change of [{ prepared_playback: "true" }, { callback_session_id: 4 }]) {
    assert.throws(() => validateSemanticExecutionDescriptor({ ...descriptor, ...change }), /boolean|without Python callbacks/);
  }
});

test("ready exposes a validated immutable build identity across the worker boundary", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  const incoming = runtimeBuildIdentity();
  worker.emit("message", workerMessage("ready", { buildIdentity: incoming }));
  const observed = await client.ready();
  assert.deepEqual(observed, incoming);
  assert.equal(Object.isFrozen(observed), true);
  assert.equal(Object.isFrozen(observed.files), true);
  for (const descriptor of Object.values(observed.files)) assert.equal(Object.isFrozen(descriptor), true);
  incoming.files.wasm.sha256 = "0".repeat(64);
  incoming.buildId = "0".repeat(64);
  assert.deepEqual(await client.ready(), runtimeBuildIdentity());
  client.terminate();
});

test("invalid readiness provenance rejects readiness and queued Run and retires its worker", async () => {
  const valid = runtimeBuildIdentity();
  for (const buildIdentity of [
    undefined, null,
    { ...valid, sourceRevision: "short" },
    { ...valid, buildId: "0".repeat(64) },
    { ...valid, files: { ...valid.files, wasm: { ...valid.files.wasm, path: "./other.wasm" } } },
    { ...valid, files: { worker: valid.files.worker, wasm: valid.files.wasm, glue: valid.files.glue } },
    { ...valid, extra: true },
  ]) {
    const worker = new FakeWorker();
    const client = new PythonAuthoringClient(worker);
    const readyRejected = assert.rejects(client.ready(), /runtime build identity|provenance|digest/);
    const runRejected = assert.rejects(client.run("result = scene"), /runtime build identity|provenance|digest/);
    worker.emit("message", workerMessage("ready", { buildIdentity }));
    await Promise.all([readyRejected, runRejected]);
    assert.equal(worker.terminated, true);
    assert.equal(client.terminated, true);
    assert.deepEqual(worker.messages, [], "invalid readiness must not submit source");
  }
});

test("termination wins while readiness identity validation is pending", async (t) => {
  const digest = deferred();
  t.mock.method(crypto.subtle, "digest", () => digest.promise);
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  const incoming = runtimeBuildIdentity();
  const rejected = assert.rejects(client.ready(), /terminated/);
  worker.emit("message", workerMessage("ready", { buildIdentity: incoming }));
  client.terminate();
  digest.resolve(Uint8Array.from(Buffer.from(incoming.buildId, "hex")).buffer);
  await rejected;
  await assert.rejects(client.run("result = scene"), /terminated/);
  assert.deepEqual(worker.messages, []);
});

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
  });

  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify(semanticResult()),
      workerRunTiming: {
        performanceTimeOriginMs: 1000,
        startedAtMs: 20,
        completedAtMs: 35,
      },
    }),
  );
  assert.deepEqual(await resultPromise, {
    ...parsedSemanticResult(),
    workerRunTiming: {
      performanceTimeOriginMs: 1000,
      startedAtMs: 20,
      completedAtMs: 35,
    },
  });
});

test("scene results without a semantic execution descriptor are rejected", () => {
  const scene = { version: 1, objects: [], tracks: [] };
  assert.throws(
    () =>
      parseAuthoringResult(
        JSON.stringify({
          kind: "semantic_scene",
          document: scene,
          duration: 0,
          identities: { objects: [], tracks: [] },
        }),
      ),
    /requires a semantic execution descriptor/,
  );
});

test("semantic execution results carry only shared execution metadata", () => {
  const result = parseAuthoringResult(
    JSON.stringify({
      kind: "semantic_scene",
      semantic_execution: { context_id: "semantic-7" },
      duration: 2,
      document: "not a legacy scene",
      scene_spec: { version: -1 },
    }),
  );
  assert.deepEqual(result, {
    kind: "semantic_scene",
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
  await client.ready();
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
  await client.ready();
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
  await client.ready();
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
  await client.ready();
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
  await client.ready();
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
    kind: "semantic_scene",
    semantic_execution: { context_id: "scene" },
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

for (const duration of [0, 1]) test(`prepared result without a continuation requires completed static duration (${duration})`, async () => {
  const worker = new FakeWorker(), client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready")); await client.ready();
  const running = client.run("static source", {}, { preparedPlayback: true, onSemanticContinuation() {} });
  await Promise.resolve();
  worker.emit("message", workerMessage("result", { requestId: 0,
    resultJson: JSON.stringify({ kind: "semantic_scene", duration,
      semantic_execution: { context_id: "static", prepared_playback: true } }),
  }));
  try {
    if (duration === 0) {
      const result = await running;
      assert.equal(result.duration, 0);
      assert.deepEqual(result.semanticExecution, { contextId: "static", preparedPlayback: true });
    } else await assert.rejects(running, /registered continuation/);
  } finally { client.terminate(); }
});

for (const [label, prepared, terminate] of [
  ["prepared source failure waits for reader adoption", true, false],
  ["termination retires a prepared error waiting for adoption", true, true],
  ["live source failure remains immediate", false, false],
]) {
  test(label, async () => {
    const worker = new FakeWorker(), client = new PythonAuthoringClient(worker);
    worker.emit("message", workerMessage("ready")); await client.ready();
    const startup = deferred(), events = [];
    const result = client.run("source", {}, { preparedPlayback: prepared,
      onSemanticContinuation: async () => { await startup.promise; events.push("adopted"); },
    });
    const rejected = assert.rejects(result, terminate ? /terminated/ : /Replay\(UnsupportedDomain\)/)
      .then(() => events.push("failed"));
    await Promise.resolve();
    worker.emit("message", workerMessage("semantic_continuation_registered", {
      requestId: 0, generation: 21, duration: 0.8,
      semanticExecution: { context_id: "partial-source", continuation_generation: 21,
        prepared_playback: prepared },
    }));
    worker.emit("message", workerMessage("error", {
      requestId: 0, message: "prepared prefix unavailable: Replay(UnsupportedDomain)",
    }));
    await new Promise(resolve => setImmediate(resolve));
    assert.deepEqual(events, prepared ? [] : ["failed"]);
    if (terminate) { client.terminate(); await rejected; }
    startup.resolve(); await rejected;
    await new Promise(resolve => setImmediate(resolve));
    assert.deepEqual(events, prepared && !terminate ? ["adopted", "failed"] : ["failed", "adopted"]);
    assert.equal(client.terminated, terminate);
    assert.equal(worker.messages.filter(message => message.type === "run").length, 1);
    client.terminate();
  });
}
