import assert from "node:assert/strict";
import test from "node:test";

import {
  AUTHORING_CHANNEL,
  AUTHORING_PROTOCOL_VERSION,
  PythonAuthoringClient,
} from "./authoring-client.js";
import { ExecutionWorkerClient } from "./execution-worker-client.js";

class FakeAuthoringWorker {
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

function authoringMessage(type, payload = {}) {
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

function sceneResult(retained = null) {
  return {
    kind: "scene_document",
    document: { version: 1, objects: [], tracks: [] },
    retained_document: retained,
    duration: 0,
    identities: { objects: [], tracks: [] },
    callbacks: null,
  };
}

function fakeCanvas() {
  return {
    clientWidth: 800,
    clientHeight: 450,
    width: 800,
    height: 450,
    className: "scene",
    id: "scene",
    replacedBy: null,
    cloneNode() {
      return fakeCanvas();
    },
    replaceWith(replacement) {
      this.replacedBy = replacement;
    },
  };
}

class FakeExecutionClient {
  constructor(canvas, kind) {
    this.canvas = canvas;
    this.kind = kind;
    this.transportMode = "transferable";
    this.terminated = false;
    this.sceneJson = null;
    this.retainedDocumentJson = null;
    this.callbacks = null;
    this.resizes = [];
  }

  async start(sceneJson, retainedOrOptions = {}, maybeOptions = {}) {
    this.sceneJson = sceneJson;
    const retained = this.kind === "retained";
    if (retained) {
      this.retainedDocumentJson = retainedOrOptions;
      this.options = maybeOptions;
    } else {
      this.options = retainedOrOptions;
    }
    return {
      engine: { retained, mixed: retained },
      render: { backend: "FakeGPU", retained, mixed: retained },
      transportMode: this.transportMode,
      session: 1,
    };
  }

  ready() {
    return Promise.resolve({ transportMode: this.transportMode, session: 1 });
  }

  async replaceScene(sceneJson, options) {
    return this.#update(sceneJson, options);
  }

  async reconcileScene(sceneJson, options) {
    return this.#update(sceneJson, options);
  }

  async #update(sceneJson, options) {
    this.sceneJson = sceneJson;
    this.options = options;
    return {
      type: "result",
      sceneJson,
      nextPatchSequence: "0",
      incremental: true,
    };
  }

  async configureHostCallbacks(callbacks) {
    this.callbacks = callbacks;
  }

  async state() {
    return {
      type: "state",
      sceneJson: this.sceneJson,
      retainedDocumentJson: this.retainedDocumentJson,
      nextPatchSequence: "0",
    };
  }

  async metrics() {
    const retained = this.kind === "retained";
    return {
      type: "metrics",
      metrics: {
        objectCount: retained ? 5 : 4,
        drawCalls: 1,
        bytesUploaded: 1,
        time: 0,
      },
      engineMetrics: retained ? { resourceBundleTransfers: 1 } : { host: { enabled: true } },
    };
  }

  async setLoopDurationSeconds(value) {
    this.loopDurationSeconds = value;
    return { type: "result", nextPatchSequence: "0" };
  }

  applyPatchBatch(json) {
    return Promise.resolve({ type: "result", json });
  }

  resize(width, height, devicePixelRatio) {
    this.resizes.push([width, height, devicePixelRatio]);
  }

  restart() {
    return Promise.resolve({
      render: { backend: "FakeGPU" },
      transportMode: this.transportMode,
      session: 2,
    });
  }

  terminate() {
    this.terminated = true;
  }
}

test("PythonAuthoringClient exposes each retained sidecar as a one-shot execution handoff", async () => {
  const worker = new FakeAuthoringWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", authoringMessage("ready"));
  await client.ready();

  const retained = retainedDocument();
  const resultPromise = client.run("result = scene");
  await Promise.resolve();
  worker.emit(
    "message",
    authoringMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify(sceneResult(retained)),
    }),
  );
  const result = await resultPromise;
  assert.deepEqual(result.retainedDocument, retained);
  assert.deepEqual(client.consumeRetainedDocument(), retained);
  assert.equal(client.consumeRetainedDocument(), null);

  const plainPromise = client.run("result = plain");
  await Promise.resolve();
  worker.emit(
    "message",
    authoringMessage("result", {
      requestId: 1,
      resultJson: JSON.stringify(sceneResult(null)),
    }),
  );
  await plainPromise;
  assert.equal(client.consumeRetainedDocument(), null);
});

test("public execution client switches retained scenes atomically and returns to legacy execution", async () => {
  const canvas = fakeCanvas();
  const clients = [];
  const client = new ExecutionWorkerClient(canvas, {
    legacyClientFactory(target) {
      const value = new FakeExecutionClient(target, "legacy");
      clients.push(value);
      return value;
    },
    retainedClientFactory(target) {
      const value = new FakeExecutionClient(target, "retained");
      clients.push(value);
      return value;
    },
  });

  await client.start('{"version":1,"objects":[],"tracks":[]}', {
    loopDurationSeconds: 4,
    transportMode: "transferable",
  });
  assert.equal(client.executionMode, "legacy");

  const retained = retainedDocument();
  let consumed = false;
  const authoringClient = {
    consumeRetainedDocument() {
      if (consumed) return null;
      consumed = true;
      return retained;
    },
  };
  const retainedResult = await client.reconcileScene(
    '{"version":1,"objects":[],"tracks":[]}',
    { authoringClient },
  );
  assert.equal(client.executionMode, "retained");
  assert.equal(retainedResult.incremental, false);
  assert.equal(JSON.parse(clients[1].retainedDocumentJson).objects.length, 1);
  const retainedMetrics = await client.metrics();
  assert.equal(retainedMetrics.metrics.objectCount, 5);
  assert.equal(retainedMetrics.engineMetrics.resourceBundleTransfers, 1);
  assert.equal(retainedMetrics.engineMetrics.host.enabled, false);
  assert.equal(clients[0].terminated, true);
  assert.notEqual(client.canvas, canvas);

  const legacyResult = await client.reconcileScene(
    '{"version":1,"objects":[],"tracks":[]}',
    { authoringClient },
  );
  assert.equal(client.executionMode, "legacy");
  assert.equal(legacyResult.incremental, false);
  assert.equal(clients[1].terminated, true);
  const legacyMetrics = await client.metrics();
  assert.equal(legacyMetrics.metrics.objectCount, 4);
  assert.equal(legacyMetrics.engineMetrics.host.enabled, true);

  client.terminate();
});
