import assert from "node:assert/strict";
import test from "node:test";

import {
  AUTHORING_CHANNEL,
  AUTHORING_PROTOCOL_VERSION,
  PythonAuthoringClient,
  validatePatchBatch,
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
    workerMessage("patch_batch", { requestId: 0, batch }),
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
