import assert from "node:assert/strict";
import test from "node:test";

import {
  AUTHORING_CHANNEL,
  AUTHORING_PROTOCOL_VERSION,
  PythonAuthoringClient,
} from "./authoring-client.js";
import { PlaygroundGeneration } from "./playground-generation.js";

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

function sceneResultJson(objectId) {
  return JSON.stringify({
    kind: "scene_document",
    document: { version: 1, objects: [{ id: objectId }], tracks: [] },
    duration: 0,
    identities: {
      objects: [{ id: objectId, key: `@object:${objectId}` }],
      tracks: [],
    },
    callbacks: null,
  });
}

function emitSceneResult(worker, requestId, objectId) {
  worker.emit(
    "message",
    workerMessage("result", {
      requestId,
      resultJson: sceneResultJson(objectId),
    }),
  );
}

test("correlates concurrent Python runs when worker results complete out of order", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const older = client.run("result = older");
  const newer = client.run("result = newer");
  await Promise.resolve();

  assert.deepEqual(
    worker.messages.map(({ requestId }) => requestId),
    [0, 1],
  );

  emitSceneResult(worker, 1, 101);
  emitSceneResult(worker, 0, 100);

  const [olderResult, newerResult] = await Promise.all([older, newer]);
  assert.equal(olderResult.document.objects[0].id, 100);
  assert.equal(newerResult.document.objects[0].id, 101);
  assert.equal(client.diagnostics.pendingRequests, 0);
  assert.equal(client.terminated, false);
});

test("playground freshness admits only the newest result under seeded out-of-order stress", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const generations = new PlaygroundGeneration();
  generations.commitSelection(generations.beginSelectionRequest("scene"));

  const requestCount = 500;
  const commits = [];
  const requests = [];
  for (let index = 0; index < requestCount; index += 1) {
    const token = generations.beginRun("scene");
    const promise = client
      .run(`result = scene_${index}`, {
        playground: {
          example_id: "scene",
          selection_generation: token.selectionGeneration,
          run_generation: token.runGeneration,
        },
      })
      .then((authored) => {
        if (!generations.isRunCurrent(token, "scene")) {
          generations.recordStale(token, "after-authoring");
          return false;
        }
        commits.push(authored.document.objects[0].id);
        return true;
      });
    requests.push(promise);
  }

  await Promise.resolve();
  assert.equal(worker.messages.length, requestCount);
  assert.equal(worker.messages[0].requestId, 0);
  assert.equal(worker.messages.at(-1).requestId, requestCount - 1);

  let seed = 0x6e6f6f6e;
  const random = () => {
    seed ^= seed << 13;
    seed ^= seed >>> 17;
    seed ^= seed << 5;
    return seed >>> 0;
  };
  const completionOrder = Array.from({ length: requestCount }, (_, index) => index);
  for (let index = completionOrder.length - 1; index > 0; index -= 1) {
    const swapIndex = random() % (index + 1);
    [completionOrder[index], completionOrder[swapIndex]] = [
      completionOrder[swapIndex],
      completionOrder[index],
    ];
  }
  for (const requestId of completionOrder) {
    emitSceneResult(worker, requestId, requestId);
  }

  const committed = await Promise.all(requests);
  assert.equal(committed.filter(Boolean).length, 1);
  assert.deepEqual(commits, [requestCount - 1]);
  assert.equal(generations.diagnostics.staleDrops, requestCount - 1);
  assert.equal(client.diagnostics.pendingRequests, 0);
  assert.equal(client.terminated, false);
});
