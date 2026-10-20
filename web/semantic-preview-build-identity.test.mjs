import assert from "node:assert/strict";
import test from "node:test";

import { SemanticPreviewSession } from "./semantic-preview-session.js";
import { AUTHORING_CHANNEL, AUTHORING_PROTOCOL_VERSION, PythonAuthoringClient } from "./authoring-client.js";
import { FakeWorker } from "./test-support/execution-fakes.mjs";
import { runtimeBuildIdentity } from "./test-support/runtime-build.mjs";

const buildIdentity = Object.freeze(runtimeBuildIdentity());

class FakeAuthoringClient {
  constructor(identity) {
    this.identity = identity;
    this.terminated = false;
  }

  async ready() {
    return this.identity;
  }

  async run(_source, _context, { onSemanticContinuation }) {
    await onSemanticContinuation({
      semanticExecution: { contextId: "semantic-1", continuationGeneration: 1 },
      generation: 1,
      duration: 1,
    });
    return { duration: 1 };
  }

  terminate() {
    this.terminated = true;
  }
}

class FakeExecutionClient {
  terminated = false;

  async prepare() {}
  async startSemanticExecution() { return { render: { time: 0 } }; }

  async advanceTo(time) {
    return { time };
  }

  async sampleToAuthoredTime(time) {
    return { time };
  }

  async metrics() {
    return { metrics: { backend: "webgl", objectCount: 1, drawCalls: 1 } };
  }

  terminate() {
    this.terminated = true;
  }
}

function session(identity) {
  return new SemanticPreviewSession({
    createAuthoringClient: () => new FakeAuthoringClient(identity),
    createExecutionClient: () => new FakeExecutionClient(),
  });
}

test("preview snapshots retain the verified authoring build identity", async () => {
  const preview = session(buildIdentity);
  const opened = await preview.open("result = Scene()");
  assert.equal(opened.buildIdentity, buildIdentity);
  const sampled = await preview.sample(0.5);
  assert.equal(sampled.buildIdentity, buildIdentity);
  assert.equal(sampled.frame.publishedTime, 0.5);
  preview.close();
});

test("a custom provenance-free client leaves optional snapshot metadata absent", async () => {
  const preview = session(undefined);
  const opened = await preview.open("result = Scene()");
  assert.equal(Object.hasOwn(opened, "buildIdentity"), false);
  preview.close();
});

test("the production authoring client carries worker identity into preview snapshots", async (t) => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  const preview = new SemanticPreviewSession({
    createAuthoringClient: () => client,
    createExecutionClient: () => new FakeExecutionClient(),
  });
  t.after(() => preview.close());
  const incoming = runtimeBuildIdentity();
  const opened = preview.open("result = Scene()");
  worker.emitMessage({ channel: AUTHORING_CHANNEL, protocolVersion: AUTHORING_PROTOCOL_VERSION,
    type: "ready", buildIdentity: incoming });
  const identity = await client.ready();
  await new Promise(setImmediate);
  assert.equal(worker.messages[0].message.type, "run");
  worker.emitMessage({ channel: AUTHORING_CHANNEL, protocolVersion: AUTHORING_PROTOCOL_VERSION,
    type: "result", requestId: worker.messages[0].message.requestId,
    resultJson: JSON.stringify({ kind: "semantic_scene", duration: 0,
      semantic_execution: { context_id: "static-preview" } }) });
  const snapshot = await opened;
  assert.equal(snapshot.buildIdentity, identity);
  incoming.files.wasm.sha256 = "0".repeat(64);
  assert.deepEqual(snapshot.buildIdentity, runtimeBuildIdentity());
  assert.equal((await preview.sample(0)).buildIdentity, identity);
  assert.equal(preview.close().buildIdentity, identity);
  assert.equal(worker.terminated, true);
});
