import assert from "node:assert/strict";
import test from "node:test";

import { AUTHORING_CHANNEL, AUTHORING_PROTOCOL_VERSION } from "./authoring-client.js";
import {
  ProvenancedPythonAuthoringClient,
  validateRuntimeBuildIdentity,
} from "./provenanced-authoring-client.js";

class FakeWorker {
  listeners = new Map();
  terminated = false;

  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  postMessage() {}

  terminate() {
    this.terminated = true;
  }

  emit(type, payload) {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(type === "message" ? { data: payload } : payload);
    }
  }
}

function identity(overrides = {}) {
  return {
    schema: 1,
    sourceRevision: "a".repeat(40),
    files: {
      worker: { path: "./python-worker.js", sha256: "b".repeat(64) },
      wasm: { path: "./pkg/noon_web_bg.wasm", sha256: "c".repeat(64) },
      glue: { path: "./pkg/noon_web.js", sha256: "d".repeat(64) },
    },
    buildId: "e".repeat(64),
    ...overrides,
  };
}

function ready(buildIdentity) {
  return {
    channel: AUTHORING_CHANNEL,
    protocolVersion: AUTHORING_PROTOCOL_VERSION,
    type: "ready",
    buildIdentity,
  };
}

test("ready resolves with a frozen verified-runtime identity observation", async () => {
  const worker = new FakeWorker();
  const client = new ProvenancedPythonAuthoringClient(worker);
  worker.emit("message", ready(identity()));
  const observed = await client.ready();
  assert.deepEqual(observed, identity());
  assert.equal(Object.isFrozen(observed), true);
  assert.equal(Object.isFrozen(observed.files), true);
  assert.equal(Object.isFrozen(observed.files.wasm), true);
});

test("malformed or missing build identity fails the provenance-aware ready boundary", async () => {
  for (const value of [
    undefined,
    null,
    identity({ buildId: "not-a-hash" }),
    identity({ sourceRevision: "short" }),
    identity({ files: { ...identity().files, wasm: { path: "https://other.invalid/noon.wasm", sha256: "c".repeat(64) } } }),
    { ...identity(), extra: true },
  ]) {
    const worker = new FakeWorker();
    const client = new ProvenancedPythonAuthoringClient(worker);
    worker.emit("message", ready(value));
    await assert.rejects(client.ready(), /build identity|build provenance/);
  }
});

test("validator copies descriptors rather than retaining a mutable message object", () => {
  const incoming = identity();
  const observed = validateRuntimeBuildIdentity(incoming);
  incoming.files.wasm.sha256 = "f".repeat(64);
  incoming.buildId = "0".repeat(64);
  assert.equal(observed.files.wasm.sha256, "c".repeat(64));
  assert.equal(observed.buildId, "e".repeat(64));
});
