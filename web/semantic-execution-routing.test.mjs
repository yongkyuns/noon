import assert from "node:assert/strict";
import test from "node:test";

import {
  FakeCanvas, FakeMessageChannel, FakeWorker, FakeResizeObserver, FakeSemanticAuthoringClient,
} from "./test-support/execution-fakes.mjs";

globalThis.ResizeObserver = FakeResizeObserver;

globalThis.HTMLCanvasElement = FakeCanvas;
globalThis.MessageChannel = FakeMessageChannel;
globalThis.Worker = FakeWorker;
globalThis.window = { devicePixelRatio: 1 };

const {
  AuthoringExecutionClient,
  AUTHORING_EXECUTION_SEMANTIC,
} = await import("./authoring-execution-client.js");

function envelope(channel, type, payload = {}) {
  return { channel, protocolVersion: 1, type, ...payload };
}

function renderWorker() {
  return FakeWorker.instances.findLast((worker) => worker.name === "noon-render");
}

function request(worker, type) {
  const entry = worker.messages.findLast(({ message }) => message.type === type);
  assert.ok(entry, `missing ${type}`);
  return entry.message;
}

async function waitForRequest(worker, type) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const entry = worker.messages.findLast(({ message }) => message.type === type);
    if (entry) return entry.message;
    await Promise.resolve();
  }
  assert.fail(`missing ${type}`);
}

function replyRender(worker, type, responseType, payload = {}) {
  const sent = request(worker, type);
  worker.emitMessage(
    envelope("noon.render", responseType, {
      requestId: sent.requestId,
      transportMode: "transferable",
      backend: "WebGL2",
      ...payload,
    }),
  );
}

async function prepare(client) {
  const prepared = client.prepare({ transportMode: "transferable" });
  const render = renderWorker();
  replyRender(render, "prepare", "prepared");
  await prepared;
  return render;
}

test("semantic startup uses context-owned ports without constructing an engine worker", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const render = await prepare(client);
  const started = client.startSemanticExecution(
    { contextId: "semantic-1" },
    { authoringClient: authoring, loopDurationSeconds: 2, initiallyPaused: true },
  );
  await Promise.resolve();
  replyRender(render, "start_engine", "engine_started", { mode: "legacy" });
  const ready = await started;

  assert.equal(client.mode, AUTHORING_EXECUTION_SEMANTIC);
  assert.equal(ready.session, 1);
  assert.equal(authoring.attachments.length, 1);
  assert.equal(authoring.attachments[0].options.session, 1);
  assert.equal(authoring.attachments[0].options.initiallyPaused, true);
  assert.equal(authoring.attachments[0].controlPort === authoring.attachments[0].renderPort, false);
  assert.deepEqual(
    FakeWorker.instances.map(({ name }) => name),
    ["noon-render"],
    "semantic execution must not construct a legacy JSON engine worker",
  );

  const paused = client.pause();
  assert.equal((await paused).playing, false);
  const sought = client.seek(1);
  assert.equal((await sought).time, 1);
  await client.advanceToWithRendererObservation(1.25);
  const observedAdvance = authoring.attachments[0].controlPort.peer.messages.findLast(
    (message) => message.type === "advance_to",
  );
  assert.equal(observedAdvance.time, 1.25);
  assert.equal(observedAdvance.observeRenderer, true);
  assert.equal((await client.resume()).playing, true);
  client.terminate();
});

test("initially paused startup rejects a source-owned continuation before attachment", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  await assert.rejects(
    client.startSemanticExecution(
      { contextId: "semantic-continuation", continuationGeneration: 7 },
      { authoringClient: authoring, initiallyPaused: true },
    ),
    /source-owned semantic continuations cannot start paused/,
  );
  assert.equal(authoring.attachments.length, 0);
  assert.deepEqual(FakeWorker.instances, []);
  client.terminate();
});

test("external sample pacing reaches the continuation endpoint as an absolute sample", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const render = await prepare(client);
  const started = client.startSemanticExecution(
    { contextId: "semantic-sampled", continuationGeneration: 8 },
    { authoringClient: authoring, loopDurationSeconds: 2, pacing: "external_samples" },
  );
  await Promise.resolve();
  replyRender(render, "start_engine", "engine_started", { mode: "legacy" });
  await started;

  assert.equal(authoring.attachments[0].options.pacing, "external_samples");
  const sampled = await client.sampleToAuthoredTime(1.25, { stopAtSourceCompletion: true });
  assert.equal(sampled.time, 0);
  const request = authoring.attachments[0].controlPort.peer.messages.findLast(
    (message) => message.type === "sample_to_authored_time",
  );
  assert.equal(request.time, 1.25);
  assert.equal(request.stopAtSourceCompletion, true);
  await assert.rejects(client.sampleToAuthoredTime(2, { stopAtSourceCompletion: "yes" }), /must be a boolean/);
  client.terminate();
});

test("semantic startup preserves shared mailbox transport options", async () => {
  FakeWorker.instances.length = 0;
  globalThis.crossOriginIsolated = true;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  try {
    const prepared = client.prepare({ transportMode: "shared", sharedSlotCapacity: 4096 });
    const render = renderWorker();
    replyRender(render, "prepare", "prepared", { transportMode: "shared" });
    await prepared;
    const started = client.startSemanticExecution(
      { contextId: "semantic-shared" },
      { authoringClient: authoring, sharedSlotCapacity: 4096 },
    );
    await Promise.resolve();
    replyRender(render, "start_engine", "engine_started", {
      mode: "legacy",
      transportMode: "shared",
    });
    const ready = await started;
    assert.equal(ready.transportMode, "shared");
    assert.deepEqual(
      {
        transportMode: authoring.attachments[0].options.transportMode,
        sharedSlotCapacity: authoring.attachments[0].options.sharedSlotCapacity,
      },
      { transportMode: "shared", sharedSlotCapacity: 4096 },
    );
  } finally {
    client.terminate();
    globalThis.crossOriginIsolated = false;
  }
});

test("semantic rerun preflights its context before switching the live renderer", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const render = await prepare(client);
  const initial = client.startSemanticExecution(
    { contextId: "semantic-1" },
    { authoringClient: authoring },
  );
  await Promise.resolve();
  replyRender(render, "start_engine", "engine_started", { mode: "legacy" });
  await initial;

  authoring.failContext = "semantic-bad";
  await assert.rejects(
    client.reconcileSemanticExecution(
      { contextId: "semantic-bad" },
      { authoringClient: authoring },
    ),
    /semantic context rejected/,
  );
  assert.equal(
    render.messages.some(({ message }) => message.type === "rebuild_engine"),
    false,
    "failed candidate must not touch the live renderer",
  );
  assert.equal(client.mode, AUTHORING_EXECUTION_SEMANTIC);

  authoring.failContext = null;
  const rerun = client.reconcileSemanticExecution(
    { contextId: "semantic-2" },
    { authoringClient: authoring },
  );
  const rebuild = await waitForRequest(render, "rebuild_engine");
  render.emitMessage(
    envelope("noon.render", "engine_rebuilt", {
      requestId: rebuild.requestId,
      mode: "legacy",
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );
  const result = await rerun;
  assert.equal(result.ready.session, 2);
  assert.equal(authoring.attachments.at(-1).options.session, 2);
  await Promise.resolve();
  assert.deepEqual(authoring.stoppedContexts, ["semantic-1"]);
  assert.deepEqual(authoring.releasedContexts, ["semantic-1"]);
  client.terminate();
});

test("a replacement source continuation keeps its generation and starts despite a paused prior scene", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const render = await prepare(client);
  const initial = client.startSemanticExecution(
    { contextId: "semantic-paused" },
    { authoringClient: authoring, initiallyPaused: true },
  );
  await Promise.resolve();
  replyRender(render, "start_engine", "engine_started", { mode: "legacy" });
  await initial;

  const replacement = client.reconcileSemanticExecution(
    { contextId: "semantic-continuation", continuationGeneration: 2 },
    { authoringClient: authoring },
  );
  const rebuild = await waitForRequest(render, "rebuild_engine");
  render.emitMessage(
    envelope("noon.render", "engine_rebuilt", {
      requestId: rebuild.requestId,
      mode: "retained",
      transportMode: "transferable",
      backend: "WebGL2",
    }),
  );
  await replacement;

  const attachment = authoring.attachments.at(-1);
  assert.equal(attachment.contextId, "semantic-continuation");
  assert.equal(attachment.options.session, 2);
  assert.equal(attachment.options.continuationGeneration, 2);
  assert.equal(attachment.options.initiallyPaused, false);
  client.terminate();
});

test("semantic renderer recovery reattaches the same token with a fresh session", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const firstRender = await prepare(client);
  const initial = client.startSemanticExecution(
    { contextId: "semantic-6" },
    { authoringClient: authoring, initiallyPaused: true },
  );
  await Promise.resolve();
  replyRender(firstRender, "start_engine", "engine_started", { mode: "legacy" });
  await initial;

  const restarting = client.restart();
  for (;;) {
    if (FakeWorker.instances.filter(({ name }) => name === "noon-render").length === 2) break;
    await Promise.resolve();
  }
  const secondRender = renderWorker();
  replyRender(secondRender, "prepare", "prepared");
  await waitForRequest(secondRender, "start_engine");
  replyRender(secondRender, "start_engine", "engine_started", { mode: "legacy" });
  const ready = await restarting;

  assert.equal(ready.session, 2);
  assert.deepEqual(
    authoring.attachments.map(({ contextId, options }) => [
      contextId, options.session, options.initiallyPaused,
    ]),
    [
      ["semantic-6", 1, true],
      ["semantic-6", 2, true],
    ],
  );
  assert.deepEqual(authoring.stoppedContexts, ["semantic-6"]);
  assert.deepEqual(authoring.releasedContexts, []);
  assert.equal(firstRender.terminated, true);
  assert.equal(secondRender.terminated, false);
  client.terminate();
});

test("terminating a semantic rerun retires both endpoints without restoring old state", async () => {
  FakeWorker.instances.length = 0;
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const render = await prepare(client);
  const initial = client.startSemanticExecution(
    { contextId: "semantic-7" },
    { authoringClient: authoring },
  );
  await Promise.resolve();
  replyRender(render, "start_engine", "engine_started", { mode: "legacy" });
  await initial;

  const transition = client.reconcileSemanticExecution(
    { contextId: "semantic-8" },
    { authoringClient: authoring },
  );
  await waitForRequest(render, "rebuild_engine");
  client.terminate();
  await assert.rejects(transition, /terminated during an asynchronous operation/);
  await Promise.resolve();
  assert.deepEqual(new Set(authoring.stoppedContexts), new Set(["semantic-7", "semantic-8"]));
  assert.deepEqual(authoring.releasedContexts, ["semantic-7"]);
  await assert.rejects(client.state(), /has not been started/);
});

test("prepared shared startup inherits slot capacity and remains unpublished until attachment", async () => {
  FakeWorker.instances.length = 0;
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const authoring = new FakeSemanticAuthoringClient();
  const sharedSlotCapacity = 2 * 1024 * 1024;
  const preparing = client.prepare({ transportMode: "transferable", sharedSlotCapacity });
  const render = renderWorker();
  replyRender(render, "prepare", "prepared");
  await preparing;
  const starting = client.startSemanticExecution({ contextId: "prepared-shared" }, { authoringClient: authoring });
  await waitForRequest(render, "start_engine");
  assert.equal(client.mode, null);
  await assert.rejects(client.state(), /has not been started/);
  assert.equal(authoring.attachments[0].options.sharedSlotCapacity, sharedSlotCapacity);
  assert.equal(FakeWorker.instances.length, 1, "only the prepared render worker is needed");
  replyRender(render, "start_engine", "engine_started", { mode: "retained" });
  await starting;
  assert.equal(client.mode, AUTHORING_EXECUTION_SEMANTIC);
  client.terminate();
});

test("terminating preparation cancels the unpublished shared candidate and replaces its transferred canvas", async () => {
  FakeWorker.instances.length = 0;
  const original = new FakeCanvas();
  const client = new AuthoringExecutionClient(original);
  const preparing = client.prepare({ transportMode: "transferable" });
  const render = renderWorker();
  client.terminate();
  await assert.rejects(preparing, /terminated during an asynchronous operation/);
  assert.equal(render.terminated, true);
  assert.equal(original.transferred, true);
  assert.equal(original.replacement, client.canvas);
  assert.notEqual(client.canvas, original);
  assert.equal(client.canvas.transferred, false);
  assert.equal(client.mode, null);
  await assert.rejects(client.state(), /has not been started/);
});

test("failed shared startup adopts a usable canvas and can retry", async () => {
  FakeWorker.instances.length = 0;
  const original = new FakeCanvas();
  const client = new AuthoringExecutionClient(original);
  const authoring = new FakeSemanticAuthoringClient();
  authoring.failContext = "rejected-start";
  const firstRender = await prepare(client);
  await assert.rejects(
    client.startSemanticExecution({ contextId: "rejected-start" }, { authoringClient: authoring }),
    /semantic context rejected/,
  );
  assert.equal(firstRender.terminated, true);
  assert.equal(client.mode, null);
  assert.notEqual(client.canvas, original);
  assert.equal(client.canvas.transferred, false);
  const render = await prepare(client);
  const started = client.startSemanticExecution({ contextId: "retry-start" }, { authoringClient: authoring });
  await waitForRequest(render, "start_engine");
  replyRender(render, "start_engine", "engine_started", { mode: "retained" });
  await started;
  assert.equal(client.mode, AUTHORING_EXECUTION_SEMANTIC);
  client.terminate();
});

test("shared recovery remains retryable after a transient render startup error", async () => {
  FakeWorker.instances.length = 0;
  const client = new AuthoringExecutionClient(new FakeCanvas());
  const authoring = new FakeSemanticAuthoringClient();
  const render = await prepare(client);
  const initial = client.startSemanticExecution({ contextId: "restart-retry" }, { authoringClient: authoring });
  await waitForRequest(render, "start_engine");
  replyRender(render, "start_engine", "engine_started", { mode: "retained" });
  await initial;
  const observer = FakeResizeObserver.instances.at(-1);
  const restarting = client.restart();
  assert.equal(observer.active, false);
  assert.doesNotThrow(() => observer.deliver(), "queued resize must wait for recovery");
  assert.doesNotThrow(() => client.resize(800, 450), "explicit resize must wait for recovery");
  const rejected = assert.rejects(restarting, /transient render error/);
  for (let attempt = 0; attempt < 20 && renderWorker() === render; attempt += 1) await Promise.resolve();
  const failed = renderWorker();
  replyRender(failed, "prepare", "error", { message: "transient render error" });
  await rejected;
  assert.equal(observer.active, false, "failed recovery must not resume automatic resize");
  const retry = client.restart();
  for (let attempt = 0; attempt < 20 && renderWorker() === failed; attempt += 1) await Promise.resolve();
  const recovered = renderWorker();
  replyRender(recovered, "prepare", "prepared");
  await waitForRequest(recovered, "start_engine");
  replyRender(recovered, "start_engine", "engine_started", { mode: "retained" });
  await retry;
  assert.equal(client.mode, AUTHORING_EXECUTION_SEMANTIC);
  assert.equal(recovered.terminated, false);
  const nextObserver = FakeResizeObserver.instances.at(-1);
  assert.equal(nextObserver.active, true);
  assert.equal(nextObserver.canvas, client.canvas);
  assert.doesNotThrow(() => nextObserver.deliver());
  assert.equal(request(recovered, "resize").width, 640);
  client.terminate();
});

test("cancelled shared preparation cannot roll back a replacement startup generation", async () => {
  const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
  const authoring = new FakeSemanticAuthoringClient();
  const client = new ExecutionWorkerClient(new FakeCanvas());
  const capture = promise => promise.catch(error => error);
  const firstPrepare = capture(client.prepare({ transportMode: "transferable" }));
  const firstStart = capture(client.startSemanticExecution("first", authoring));
  client.terminate();
  const nextPrepare = capture(client.prepare({ transportMode: "transferable" }));
  const replacement = renderWorker();
  const nextStart = capture(client.startSemanticExecution("replacement", authoring));
  try {
    for (const error of await Promise.all([firstPrepare, firstStart])) {
      assert.match(error.message, /terminated/);
    }
    assert.equal(replacement.terminated, false, "stale failure must not destroy the new renderer");
    await assert.rejects(client.startSemanticExecution("overlapping", authoring), /already started/);
    replyRender(replacement, "prepare", "prepared");
    await nextPrepare;
    await waitForRequest(replacement, "start_engine");
    replyRender(replacement, "start_engine", "engine_started");
    const ready = await nextStart;
    assert.equal(ready.engine.semantic, true);
    assert.deepEqual(authoring.attachments.map(entry => entry.contextId), ["replacement"]);
  } finally {
    client.terminate();
    await Promise.all([nextPrepare, nextStart]);
  }
});
