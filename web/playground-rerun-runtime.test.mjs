import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { PlaygroundGeneration } from "./playground-generation.js";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");

function section(startMarker, endMarker) {
  const start = main.indexOf(startMarker);
  const end = main.indexOf(endMarker, start);
  assert.ok(start >= 0 && end > start, `missing playground boundary: ${startMarker}`);
  return main.slice(start, end);
}

// Execute the production handlers, not a second implementation of their policy.
// Only DOM and worker lifecycles are faked; generation checks use the real class.
const handlers = [
  section("function discardEarlyContinuationRuntime(", "function sameSemanticContinuation("),
  section('resetButton.addEventListener("click"', 'sceneSourceEditor.addEventListener(\n  "focus"'),
  section('sceneSourceEditor.addEventListener("input"', 'window.addEventListener("popstate"'),
].join("\n");

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

const flush = () => new Promise((resolve) => setImmediate(resolve));

function harness() {
  const events = [];
  const listeners = {};
  const cancellation = deferred();
  const prior = deferred();
  const generations = new PlaygroundGeneration();
  const example = { id: "test-scene", title: "Test scene" };
  const selection = generations.beginSelectionRequest(example.id);
  generations.commitSelection(selection);
  const runToken = generations.beginRun(example.id);
  const oldPlayer = { terminate: () => events.push("terminate-player") };
  const client = {
    terminated: false,
    cancelSemanticContinuation(...args) {
      assert.equal(generations.isRunCurrent(runToken, example.id), false,
        "invalidate before cancellation can deliver any old worker result");
      events.push(["cancel", ...args]);
      return cancellation.promise;
    },
    terminate() {
      this.terminated = true;
      events.push("terminate-authoring");
    },
  };
  const continuation = {
    client,
    attachedPlayer: oldPlayer,
    runToken,
    registration: { semanticExecution: { contextId: "old-context", continuationGeneration: 9 } },
  };
  const context = {
    generations,
    sceneRunPromise: prior.promise,
    runTransitionPromise: null,
    activeSourceContinuation: continuation,
    authoringClient: client,
    player: oldPlayer,
    playerNeedsRestart: false,
    playbackControls: { destroy: () => events.push("destroy-controls") },
    patchStatus: { value: "Playing", dataset: { state: "running" } },
    sceneSourceEditor: {
      value: "original source",
      addEventListener: (name, handler) => { listeners[name] = handler; },
    },
    resetButton: {
      disabled: false,
      addEventListener: (_name, handler) => { listeners.reset = handler; },
    },
    canonicalSource: "original source",
    currentExample: () => example,
    drafts: new Map(),
    adoptRuntimeCanvas: (candidate) => {
      assert.equal(candidate, oldPlayer);
      events.push("adopt-canvas");
    },
    stopMetricsPolling: () => events.push("stop-metrics"),
    runScene: () => {
      assert.equal(context.sceneRunPromise, null, "old source must unwind before reuse");
      assert.equal(context.player, null, "old runtime must retire before replacement");
      assert.equal(context.runTransitionPromise, null);
      const source = context.sceneSourceEditor.value;
      generations.beginRun(example.id);
      events.push(["run", source]);
      return { source };
    },
  };
  runInNewContext(handlers, context);
  return {
    context, events, listeners, cancellation, prior, runToken, example, oldPlayer, client,
    unwind() {
      context.sceneRunPromise = null;
      prior.resolve({ stale: true });
    },
  };
}

test("typing and Reset preserve the active source run and perform no execution action", () => {
  const h = harness();
  const { context } = h;
  context.sceneSourceEditor.value = "edited source";
  h.listeners.input();
  assert.equal(context.drafts.get(h.example.id), "edited source");
  assert.equal(context.resetButton.disabled, false);
  assert.match(context.patchStatus.value, /current preview continues · Run to apply/);
  assert.equal(context.generations.isRunCurrent(h.runToken, h.example.id), true);
  assert.equal(context.player, h.oldPlayer);
  assert.equal(context.sceneRunPromise, h.prior.promise);
  assert.deepEqual(h.events, []);

  h.listeners.reset();
  assert.equal(context.sceneSourceEditor.value, context.canonicalSource);
  assert.equal(context.drafts.has(h.example.id), false);
  assert.equal(context.resetButton.disabled, true);
  assert.equal(context.generations.isRunCurrent(h.runToken, h.example.id), true);
  assert.equal(context.player, h.oldPlayer);
  assert.equal(context.sceneRunPromise, h.prior.promise);
  assert.deepEqual(h.events, []);
});

test("Run cancels immediately, waits only for cancellation cleanup, then runs the latest draft", async () => {
  const h = harness();
  h.context.sceneSourceEditor.value = "edited source";
  const rerun = h.context.requestSceneRun();
  assert.deepEqual(h.events, [["cancel", "old-context", 9, "Superseded by an explicit playground Run"]]);
  assert.equal(h.context.generations.isRunCurrent(h.runToken, h.example.id), false);
  assert.equal(h.context.player, h.oldPlayer, "retain ownership until cancellation acknowledges");

  h.cancellation.resolve();
  await flush();
  assert.equal(h.context.player, null);
  assert.equal(h.context.activeSourceContinuation, null);
  assert.equal(h.events.some((event) => Array.isArray(event) && event[0] === "run"), false);

  // No animation-completed event is delivered. Only cancellation unwind releases
  // the interpreter, and edits made while it unwinds must be the next Run's input.
  h.context.sceneSourceEditor.value = "latest draft";
  h.unwind();
  assert.deepEqual(await rerun, { source: "latest draft" });
  assert.equal(h.context.authoringClient, h.client, "successful cancellation reuses the worker");
  assert.equal(h.client.terminated, false);
  assert.deepEqual(h.events.slice(1), [
    "terminate-player", "adopt-canvas", "destroy-controls", "stop-metrics", ["run", "latest draft"],
  ]);
});

test("failed cancellation retires its authoring worker and still releases the rerun transition", async () => {
  const h = harness();
  const rerun = h.context.requestSceneRun();
  h.cancellation.reject(new Error("continuation already retired"));
  await flush();
  assert.equal(h.client.terminated, true);
  assert.equal(h.context.authoringClient, null);
  assert.equal(h.context.player, null);
  h.unwind();
  assert.deepEqual(await rerun, { source: "original source" });
  assert.equal(h.context.runTransitionPromise, null);
  assert.equal(h.events.filter((event) => event === "terminate-authoring").length, 1);
});

test("late cancellation cleanup cannot clear a replacement's client, player, or continuation", async () => {
  const h = harness();
  const cancelling = h.context.supersedeActiveSourceContinuation();
  const replacementClient = { terminated: false, terminate: () => assert.fail("replacement client retired") };
  const replacementPlayer = { terminate: () => assert.fail("replacement player retired") };
  const replacementContinuation = { client: replacementClient, attachedPlayer: replacementPlayer };
  h.context.authoringClient = replacementClient;
  h.context.player = replacementPlayer;
  h.context.activeSourceContinuation = replacementContinuation;
  h.cancellation.reject(new Error("late cancellation rejection"));
  assert.equal(await cancelling, true);
  assert.equal(h.client.terminated, true);
  assert.equal(h.context.authoringClient, replacementClient);
  assert.equal(h.context.player, replacementPlayer);
  assert.equal(h.context.activeSourceContinuation, replacementContinuation);
  assert.deepEqual(h.events.slice(1), ["terminate-authoring", "terminate-player"]);
});
