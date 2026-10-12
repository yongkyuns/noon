import assert from "node:assert/strict";
import test from "node:test";

import {
  AUTHORING_CHANNEL,
  AUTHORING_PROTOCOL_VERSION,
  PythonAuthoringClient,
} from "./authoring-client.js";
import { PlaygroundGeneration } from "./playground-generation.js";
import { createRunRequestRouter } from "./playground-run-request-router.js";
import { createSourceRestart } from "./playground-source-restart.js";

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
  return JSON.stringify({ kind: "semantic_scene", semantic_execution: { context_id: String(objectId) }, duration: 0 });
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
  assert.equal(Number(olderResult.semanticExecution.contextId), 100);
  assert.equal(Number(newerResult.semanticExecution.contextId), 101);
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
        commits.push(Number(authored.semanticExecution.contextId));
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

  const postStress = client.run("result = post_stress");
  await Promise.resolve();
  assert.equal(worker.messages.at(-1).requestId, requestCount);
  emitSceneResult(worker, requestCount, requestCount);
  const postStressResult = await postStress;
  assert.equal(Number(postStressResult.semanticExecution.contextId), requestCount);
  assert.deepEqual(client.diagnostics, {
    nextRequestId: requestCount + 1,
    pendingRequests: 0,
    staleResponses: 0,
    terminated: false,
  });
});

test("newest edit wins while the automatic authoring preload is in flight", async () => {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.emit("message", workerMessage("ready"));
  await client.ready();

  const generations = new PlaygroundGeneration();
  generations.commitSelection(generations.beginSelectionRequest("scene"));
  const timers = new Map();
  let nextTimer = 0;
  let source = "preload source";
  let active = null;
  const presented = [];
  const startRun = () => {
    const token = generations.beginRun("scene");
    const requestSource = source;
    const promise = client.run(requestSource, {
      playground: {
        example_id: "scene",
        selection_generation: token.selectionGeneration,
        run_generation: token.runGeneration,
      },
    }).then(() => {
      if (!generations.isRunCurrent(token, "scene")) {
        generations.recordStale(token, "after-authoring");
        return { stale: true };
      }
      presented.push({ source: requestSource, generation: token.runGeneration });
      return { stale: false };
    }).finally(() => {
      if (active?.promise === promise) active = null;
    });
    active = { promise, token, source: requestSource };
    return promise;
  };
  const router = createRunRequestRouter({
    currentSource: () => source,
    currentRun: () => active?.promise ?? null,
    activeSourceContinuation: () => null,
    activeRunRequest: () => active,
    isActiveRunCurrent: (token) => active?.token === token
      && generations.isRunCurrent(token, "scene"),
    run: startRun,
    async supersede() { return false; },
    onQueued() {},
  });
  const restart = createSourceRestart({
    stop() {
      generations.invalidateRun();
      return active?.promise ?? Promise.resolve();
    },
    run: (isCurrent) => router.request(isCurrent),
    currentSelection: () => "scene",
    onError(error) { throw error; },
    setTimer(callback) { const id = ++nextTimer; timers.set(id, callback); return id; },
    clearTimer(id) { timers.delete(id); },
  });

  // Start the same source-restart path used by the automatic preload.
  const preload = restart.runNow();
  for (let index = 0; index < 8; index += 1) await Promise.resolve();
  assert.equal(worker.messages.length, 1);
  assert.equal(worker.messages[0].source, "preload source");

  source = "first edit";
  restart.edited();
  source = "newest edit";
  restart.edited();
  assert.equal(timers.size, 1, "rapid edits should leave only the latest debounce request");

  // Complete the old authoring response after both edits. Its generation has
  // already been invalidated and must never cross the guarded handoff.
  emitSceneResult(worker, worker.messages[0].requestId, 100);
  await preload;
  assert.deepEqual(presented, [], "the invalidated preload must not publish after either edit");
  assert.deepEqual(worker.messages.map(({ source: requestedSource }) => requestedSource), ["preload source"]);
  const latestTimer = [...timers.values()][0];
  timers.clear();
  latestTimer();
  for (let index = 0; index < 8; index += 1) await Promise.resolve();
  assert.equal(worker.messages.length, 2);
  assert.equal(worker.messages[1].source, "newest edit");

  emitSceneResult(worker, worker.messages[1].requestId, 101);
  for (let index = 0; index < 8; index += 1) await Promise.resolve();

  assert.deepEqual(
    worker.messages.map(({ source: requestedSource }) => requestedSource),
    ["preload source", "newest edit"],
    "the two rapid edits must coalesce without dispatching the obsolete intermediate source",
  );
  assert.deepEqual(presented.map(({ source: presentedSource }) => presentedSource), ["newest edit"]);
  assert.equal(presented[0].generation, generations.diagnostics.runGeneration);
  assert.equal(generations.diagnostics.staleDrops, 1);
  restart.dispose();
});
