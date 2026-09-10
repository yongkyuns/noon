import assert from "node:assert/strict";
import test from "node:test";

import { SemanticPreviewSession } from "./semantic-preview-session.js";

const buildIdentity = Object.freeze({
  schema: 1,
  sourceRevision: "a".repeat(40),
  files: Object.freeze({
    worker: Object.freeze({ path: "./python-worker.js", sha256: "b".repeat(64) }),
    wasm: Object.freeze({ path: "./pkg/noon_web_bg.wasm", sha256: "c".repeat(64) }),
    glue: Object.freeze({ path: "./pkg/noon_web.js", sha256: "d".repeat(64) }),
  }),
  buildId: "e".repeat(64),
});

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
  async startSemanticExecution() {}

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

test("legacy or test authoring clients without provenance preserve the old snapshot shape", async () => {
  const preview = session(undefined);
  const opened = await preview.open("result = Scene()");
  assert.equal(Object.hasOwn(opened, "buildIdentity"), false);
  preview.close();
});
