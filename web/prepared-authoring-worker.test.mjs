import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { createPreparedInterpreterDriver } from "./prepared-interpreter-driver.js";

const source = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");
const section = (start, end) => {
  const begin = source.indexOf(start), finish = source.indexOf(end, begin);
  assert.ok(begin >= 0 && finish > begin);
  return source.slice(begin, finish);
};
const lifecycle = [
  section("function registerSemanticExecution", "function continuationEvent"),
  section("function continuationEvent", "function failContinuation("),
  section("function failContinuation(", "function isContinuationControl"),
  section("function isContinuationControl", "function retireSemanticContext"),
  section("function retireSemanticContext", "async function prepareAuthoringDependencies"),
  section("function validateRequest", "function post(type,"),
  source.slice(source.indexOf("function isRecord(value)")),
].join("\n");

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

async function microtasks() {
  for (let i = 0; i < 8; i++) await Promise.resolve();
}

function host({ demand = false, attachGate = null, completeError = null, trapAt = null, initialIntervals = [true], observed = false, late = false, terminalNative = false, staticNative = false } = {}) {
  const events = [], tasks = new Map(), posts = [], errors = [];
  let taskId = 0, api, execute;
  const counters = { initial: 0, segments: 0, terminal: 0, adopted: 0, reserved: 0, stopped: 0, freed: 0, delivered: 0 };
  let pending, freed = false;
  const alive = () => assert.equal(freed, false, "retired WASM handle must never be reused");
  const trap = operation => { if (operation === trapAt) throw new WebAssembly.RuntimeError(`trap during ${operation}`); };
  const player = {
    resetReceiver(session) { alive(); assert.ok(session > 3); events.push("reset-receiver"); },
    preparedPlaying: () => false,
    prepareSourceSegment(value) {
      if (late) { events.push("require-source"); return false; }
      alive(); trap("segment"); assert.equal(value, context); counters.segments++; pending = 4; events.push("segment");
    },
    beginSourceExecution(value) { alive(); assert.equal(value, context); observed = true; events.push("handoff"); return false; },
    finishSourcePreparation(value) {
      alive(); trap("terminal"); assert.equal(value, context); counters.terminal++; pending = 4; events.push("terminal");
      if (terminalNative) { pending = undefined; return false; }
    },
    reserveSourcePrefixCapacity() { alive(); trap("reserve"); if (pending === undefined) return false; assert.equal(pending, 4); counters.reserved++; return true; },
    deliverSourcePrefix() { alive(); trap("adopt"); assert.equal(pending, 4); pending = undefined; counters.adopted++; return true; },
    needsPreparation() { alive(); trap("demand"); return demand; },
    stopSourcePreparation() { alive(); counters.stopped++; pending = undefined; },
    free() { alive(); freed = true; counters.freed++; events.push("free"); },
  };
  const context = {
    beginObservedPreparedSource() { return observed; },
    prepareExecutionRun() {}, liveHandoffDuration: () => 1,
    createFinishedPreparedExecutionPlayer(session) {
      assert.equal(session, 3); events.push("static-reader");
      if (staticNative) { observed = true; return undefined; }
      return player;
    },
    retireExecutionSource() { events.push("retire-source"); },
    prepareInitialSourceSegment() { counters.initial++; events.push("initial"); trap("initial"); if (completeError) throw completeError; return initialIntervals.shift() ?? true; },
    createPreparedExecutionPlayer(session) { trap("reader"); assert.equal(session, 3); events.push("reader"); return player; },
  };
  let endpoint;
  const attachSemanticEngine = async (_context, _request, onStop, ...args) => {
    assert.equal(_context, context);
    const prepared = args[5] !== undefined;
    if (staticNative && observed) {
      assert.equal(_request.completedStaticSource, true);
      assert.equal(args[1], null, "completed static source has no live continuation protocol");
      assert.equal(prepared, false, "original canonical runtime owns input");
    }
    if (!observed) {
      assert.deepEqual(args.slice(0, 5), [null, null, null, null, null]);
      assert.equal(args[5].player, player);
    }
    let stopped = false;
    endpoint = {
      completed: args[1]?.completed === true,
      startContinuation(generation) { events.push("observe-segment"); assert.equal(generation, args[1].generation); },
      async publishContinuationResult(generation) { assert.equal(generation, args[1].generation); events.push("final-publication"); },
      sourcePrefixDelivered() { assert.equal(stopped, false); counters.delivered++; args[5].wakeSource(); },
      sourceExecutionRequired() { assert.equal(stopped, false); events.push("source-demand"); },
      detachPreparedReceiver() {
        assert.equal(stopped, false); stopped = true;
        events.push("detach-receiver"); onStop(player); return args[5];
      },
      stop() {
        if (stopped) return;
        stopped = true;
        events.push("stop"); onStop();
        if (prepared) { player.stopSourcePreparation(); player.free(); }
      },
    };
    try {
      if (attachGate && (!(late || terminalNative) || !prepared)) await attachGate.promise;
    } catch (error) {
      if (prepared && args[5].retainOnSetupFailure) { stopped = true; onStop(player); }
      else endpoint.stop();
      throw error;
    }
    return endpoint;
  };
  api = new Function("createPreparedInterpreterDriver", "runAuthoringSource", "attachSemanticEngine", "posts", "errors", `
    const AUTHORING_CHANNEL = "noon.authoring", AUTHORING_PROTOCOL_VERSION = 7;
    const pyodidePromise = Promise.resolve({}), semanticContexts = new Map();
    let nextSemanticContext = 0, nextContinuationGeneration = 1, activeAuthoringRun = null;
    let fatalAuthoringFailure = false;
    let requestQueue = Promise.resolve();
    const post = (type, value) => { if (!fatalAuthoringFailure) posts.push({ type, ...value }); };
    const postError = (requestId, error) => {
      if (!fatalAuthoringFailure || requestId === null) errors.push({ requestId, error });
    };
    function failAuthoringWorker(error) {
      if (fatalAuthoringFailure) return;
      fatalAuthoringFailure = true; postError(null, error);
    }
    ${lifecycle}
    return { registerSemanticExecution, failAuthoringWorker, awaitSemanticContinuation, handleRequest, handleContinuationControl,
      completeContinuation, requestContinuationCallback, completeContinuationCallback,
      failContinuation, contexts: semanticContexts, run: () => activeAuthoringRun,
      fatal: () => fatalAuthoringFailure };
  `)(options => createPreparedInterpreterDriver({ ...options,
    setTimeout(callback) { const id = ++taskId; tasks.set(id, callback); return id; },
    clearTimeout(id) { tasks.delete(id); },
  }), () => execute(api), attachSemanticEngine, posts, errors);
  const envelope = (type, fields) => ({ channel: "noon.authoring", protocolVersion: 7, type, ...fields });
  const continuationRequest = (type, fields = {}) => {
    const registration = posts.findLast(post => post.type === "semantic_continuation_registered");
    return envelope(type, { requestId: 2, contextId: registration.semanticExecution.context_id,
      continuationGeneration: registration.generation, continuationRunRequestId: registration.requestId, ...fields });
  };
  return {
    ...api, context, player, counters, events, tasks, posts, errors,
    setAttachGate(value) { attachGate = value; },
    async recover(extra = {}) {
      const contextId = posts.findLast(post => post.type === "semantic_continuation_registered")?.semanticExecution.context_id
        ?? JSON.parse(posts.findLast(post => post.type === "result").resultJson).semantic_execution.context_id;
      const control = new MessageChannel(), render = new MessageChannel();
      try {
        return await api.handleContinuationControl(envelope("attach_semantic_execution", {
          requestId: 4, contextId, controlPort: control.port1, renderPort: render.port1,
          session: 4, transportMode: "transferable", sharedSlotCapacity: 1024,
          loopDurationSeconds: 1, initiallyPaused: true, pacing: "realtime",
          replaceExistingEndpoint: true, ...extra,
        }));
      } finally { control.port1.close(); control.port2.close(); render.port1.close(); render.port2.close(); }
    },
    start(sourceBody, requestId = 1) {
      execute = sourceBody;
      return api.handleRequest(envelope("run", { requestId, source: "test source", context: {},
        preparedPlayback: true, preparationLookaheadSeconds: 0.75 }));
    },
    async attach(extra = {}) {
      const control = new MessageChannel(), render = new MessageChannel();
      try {
        return await api.handleContinuationControl(continuationRequest("attach_semantic_execution", {
          controlPort: control.port1, renderPort: render.port1, session: 3,
          transportMode: "transferable", sharedSlotCapacity: 1024, loopDurationSeconds: 1,
          initiallyPaused: true, pacing: "realtime", ...extra,
        }));
      } finally { control.port1.close(); control.port2.close(); render.port1.close(); render.port2.close(); }
    },
    async attachFinished(extra = {}) {
      const control = new MessageChannel(), render = new MessageChannel();
      try {
        const token = JSON.parse(posts.findLast(post => post.type === "result").resultJson).semantic_execution.context_id;
        return await api.handleRequest(envelope("attach_semantic_execution", {
          requestId: 2, contextId: token, controlPort: control.port1, renderPort: render.port1,
          session: 3, transportMode: "transferable", sharedSlotCapacity: 1024,
          loopDurationSeconds: 1, initiallyPaused: true, pacing: "realtime", ...extra,
        }));
      } finally { control.port1.close(); control.port2.close(); render.port1.close(); render.port2.close(); }
    },
    releaseFinished() {
      const token = JSON.parse(posts.findLast(post => post.type === "result").resultJson).semantic_execution.context_id;
      return api.handleContinuationControl(envelope("release_semantic_execution", { requestId: 3, contextId: token }));
    },
    cancel() { return api.handleContinuationControl(continuationRequest("cancel_semantic_continuation", { reason: "cancelled by test" })); },
    tick() {
      const [id, callback] = tasks.entries().next().value ?? [];
      assert.equal(typeof callback, "function", "expected one queued source turn");
      tasks.delete(id); return callback();
    },
    demand(value) { demand = value; api.run()?.preparedDriver.wake(); },
    result(contextId = api.run().continuation.contextId) {
      return JSON.stringify({ kind: "semantic_scene", duration: 2,
        semantic_execution: { context_id: contextId,
          continuation_generation: api.run().continuation.generation, callback_session_id: null } });
    },
    endpoint: () => endpoint,
  };
}

test("production source completes each barrier once, sleeps on Rust demand and seals after cleanup", async () => {
  const h = host(); let statements = 0;
  const done = h.start(async api => {
    await api.awaitSemanticContinuation(h.context); statements++;
    await api.awaitSemanticContinuation(h.context); statements++;
    h.events.push("cleanup"); return h.result();
  });
  await microtasks();
  assert.equal(h.posts[0].semanticExecution.prepared_playback, true);
  const initial = h.tick();
  await h.attach(); await initial;
  assert.equal(h.counters.initial, 1);
  assert.equal(statements, 0);
  assert.equal(h.tasks.size, 0);
  h.demand(false); assert.equal(h.tasks.size, 0);
  h.demand(true); await h.tick(); await microtasks();
  assert.equal(statements, 1);
  h.demand(false); await h.tick();
  assert.equal(h.counters.segments, 1);
  assert.equal(h.counters.adopted, 1);
  assert.equal(h.counters.delivered, 1, "future adoption does not await a presentation receipt");
  assert.equal(h.tasks.size, 0);
  h.demand(true); await h.tick(); await done;
  assert.equal(statements, 2);
  assert.equal(h.counters.terminal, 1);
  assert.equal(h.counters.adopted, 2);
  assert.ok(h.events.indexOf("cleanup") < h.events.indexOf("terminal"));
  assert.equal(h.errors.length, 0);
  const result = h.posts.find(post => post.type === "result");
  assert.equal(JSON.parse(result.resultJson).semantic_execution.prepared_playback, true);
  h.endpoint().stop();
});

test("receiver replacement retains the original source before and after completion", async () => {
  const h = host(); let invocations = 0, completed = 0;
  const done = h.start(async api => {
    invocations++; await api.awaitSemanticContinuation(h.context); completed++; return h.result();
  });
  await microtasks(); const initial = h.tick(); await h.attach(); await initial;
  const original = h.run().continuation.preparedPlayer;
  await h.recover();
  assert.equal(h.run().continuation.preparedPlayer, original);
  assert.equal(h.counters.freed, 0); assert.equal(completed, 0);
  h.demand(true); await h.tick(); await done;
  await h.recover({ session: 5 });
  assert.equal(invocations, 1); assert.equal(completed, 1);
  assert.equal(h.counters.initial, 1); assert.equal(h.counters.terminal, 1);
  assert.equal(h.events.filter(event => event === "reset-receiver").length, 2);
  assert.deepEqual(h.errors, []);
  h.endpoint().stop(); assert.equal(h.counters.freed, 1); assert.equal(h.contexts.size, 0);
});

test("failed replacement setup retains static history for a newer receiver retry", async () => {
  const h = host(); let invocations = 0;
  await h.start(async () => { invocations++; return staticResult(h); });
  await h.attachFinished();
  const gate = deferred(); h.setAttachGate(gate);
  const failed = h.recover(); await microtasks();
  gate.reject(new Error("replacement setup failed")); await failed;
  assert.match(String(h.errors.at(-1).error), /replacement setup failed/);
  assert.equal(h.counters.freed, 0); assert.equal(h.contexts.size, 1);
  h.setAttachGate(null); await h.recover({ session: 5 });
  assert.equal(invocations, 1);
  assert.equal(h.events.filter(event => event === "static-reader").length, 1);
  h.endpoint().stop(); assert.equal(h.counters.freed, 1); assert.equal(h.contexts.size, 0);
});

test("cancellation during replacement waits for the candidate before freeing the reader", async () => {
  const h = host(); let after = 0;
  const done = h.start(async api => { await api.awaitSemanticContinuation(h.context); after++; return h.result(); });
  await microtasks(); const initial = h.tick(); await h.attach(); await initial;
  const gate = deferred(); h.setAttachGate(gate);
  const recovering = h.recover(); await microtasks();
  await h.cancel(); await done;
  assert.equal(h.counters.freed, 0, "in-flight setup still owns the handle");
  assert.equal(h.contexts.size, 1);
  gate.resolve(); await recovering;
  assert.equal(after, 0); assert.equal(h.counters.freed, 1); assert.equal(h.contexts.size, 0);
  assert.equal(h.posts.some(post => post.type === "semantic_execution_attached" && post.requestId === 4), false);
});

test("cancellation before attachment rejects once and admits a fresh source", async () => {
  const h = host(); let cleanup = 0, after = 0;
  const done = h.start(async api => {
    try { await api.awaitSemanticContinuation(h.context); after++; }
    finally { cleanup++; }
    return h.result();
  });
  await microtasks(); await h.cancel(); await done;
  assert.equal(cleanup, 1); assert.equal(after, 0);
  assert.equal(h.counters.initial, 0); assert.equal(h.tasks.size, 0);
  assert.equal(h.errors.filter(error => error.requestId === 1).length, 1);
  assert.equal(h.contexts.size, 0);
  const fresh = h.start(async api => { await api.awaitSemanticContinuation(h.context); return h.result(); }, 5);
  await microtasks();
  const initial = h.tick(); await h.attach(); await initial;
  h.demand(true); await h.tick(); await fresh;
  assert.equal(h.posts.filter(post => post.type === "result").length, 1);
  assert.equal(h.counters.initial, 1); h.endpoint().stop();
});

test("cancellation during asynchronous attachment frees once and never publishes a suffix", async () => {
  const gate = deferred(), h = host({ attachGate: gate }); let after = 0;
  const done = h.start(async api => { await api.awaitSemanticContinuation(h.context); after++; return h.result(); });
  await microtasks(); const initial = h.tick(), attaching = h.attach();
  await microtasks(); assert.equal(h.counters.initial, 1);
  await h.cancel(); await done;
  gate.resolve(); await attaching; await initial;
  assert.equal(after, 0); assert.equal(h.counters.freed, 1);
  assert.equal(h.counters.delivered, 0); assert.equal(h.counters.terminal, 0);
  assert.equal(h.posts.filter(post => post.type === "semantic_execution_attached").length, 0);
  assert.equal(h.contexts.size, 0);
});

test("cancellation while demand is satisfied unwinds without re-completing source", async () => {
  const h = host(); let cleanup = 0;
  const done = h.start(async api => {
    try { await api.awaitSemanticContinuation(h.context); return h.result(); }
    finally { cleanup++; }
  });
  await microtasks(); const initial = h.tick(); await h.attach(); await initial;
  await h.cancel(); await done;
  assert.equal(cleanup, 1); assert.equal(h.counters.initial, 1);
  assert.equal(h.counters.segments, 0); assert.equal(h.counters.terminal, 0);
  assert.equal(h.counters.freed, 1); assert.equal(h.tasks.size, 0);
});

test("endpoint stop retires its source token after an earlier active-source release was rejected", async () => {
  const h = host();
  const done = h.start(async api => { await api.awaitSemanticContinuation(h.context); return h.result(); });
  await microtasks(); const initial = h.tick(); await h.attach(); await initial;
  const contextId = h.run().continuation.contextId;
  await h.handleContinuationControl({ channel: "noon.authoring", protocolVersion: 7,
    type: "release_semantic_execution", requestId: 9, contextId });
  assert.match(String(h.errors[0].error), /cannot release an active/);
  assert.equal(h.contexts.has(contextId), true);
  h.endpoint().stop(); await done;
  assert.equal(h.contexts.size, 0);
  assert.equal(h.counters.freed, 1);
  assert.equal(h.tasks.size, 0);
});

test("source failure retains the delivered reader and never advertises a terminal endpoint", async () => {
  const h = host({ demand: true });
  const done = h.start(async api => {
    await api.awaitSemanticContinuation(h.context);
    throw new Error("intentional source failure");
  });
  await microtasks(); const initial = h.tick(); await h.attach(); await initial; await done;
  assert.equal(h.errors.filter(error => /intentional source failure/.test(String(error.error))).length, 1);
  assert.equal(h.counters.terminal, 0); assert.equal(h.counters.freed, 0);
  const [entry] = h.contexts.values();
  assert.equal(entry.released, false); assert.equal(entry.endpoints.size, 1);
  h.endpoint().stop(); assert.equal(h.counters.freed, 1);
});

test("a foreign result cannot seal the original source even after valid delivery", async () => {
  const h = host({ demand: true });
  const done = h.start(async api => { await api.awaitSemanticContinuation(h.context); return h.result("other-context"); });
  await microtasks(); const initial = h.tick(); await h.attach(); await initial; await done;
  assert.match(String(h.errors[0].error), /original context/);
  assert.equal(h.counters.terminal, 0); assert.equal(h.counters.freed, 0);
  h.endpoint().stop();
});

test("unregistered result cannot manufacture completed static admission", async () => {
  const h = host();
  await h.start(async () => JSON.stringify({ kind: "semantic_scene", semantic_execution: { context_id: "other" } }));
  assert.match(String(h.errors[0].error), /no readable play or wait interval/);
  assert.equal(h.counters.initial, 0); assert.equal(h.counters.terminal, 0);
});

test("first-segment rejection fails attachment and source without retrying completion", async () => {
  const h = host({ completeError: new Error("unsupported prepared source") });
  const done = h.start(async api => { await api.awaitSemanticContinuation(h.context); return h.result(); });
  await microtasks(); const initial = h.tick(); await h.attach(); await initial; await done;
  assert.equal(h.counters.initial, 1); assert.equal(h.counters.freed, 0);
  assert.equal(h.errors.filter(error => /unsupported prepared source/.test(String(error.error))).length, 2);
  assert.equal(h.contexts.size, 0);
});

for (const cancelDuringAttach of [false, true]) test(`late source handoff retains the original suspension (cancel=${cancelDuringAttach})`, async () => {
  const gate = cancelDuringAttach ? deferred() : null;
  const h = host({ late: true, demand: true, attachGate: gate });
  let invocations = 0, resumed = 0;
  const done = h.start(async api => {
    invocations++;
    await api.awaitSemanticContinuation(h.context);
    const event = JSON.parse(await api.awaitSemanticContinuation(h.context));
    assert.equal(event.kind, "complete"); resumed++;
    return h.result();
  });
  await microtasks();
  const initial = h.tick(); await h.attach(); await initial; await microtasks();
  const previous = h.endpoint(), continuation = h.run().continuation;
  const pending = h.tick(); await microtasks();
  assert.deepEqual(h.events.slice(-2), ["require-source", "source-demand"]);
  assert.equal(resumed, 0);
  assert.equal(continuation.pending !== null, true);
  const attachment = h.attach({ sourceHandoff: true });
  if (cancelDuringAttach) {
    await microtasks(); await h.cancel(); await done;
    assert.equal(h.contexts.size, 1, "registry must retain the in-flight source lease");
    gate.resolve(); await attachment; await pending;
    assert.equal(h.contexts.size, 0);
    assert.equal(resumed, 0);
  } else {
    await attachment;
    previous.stop();
    assert.equal(continuation.terminal, false, "retiring the reader must not cancel its successor");
    h.completeContinuation(continuation, continuation.generation);
    await pending; await done;
    assert.equal(resumed, 1);
    assert.equal(h.errors.length, 0);
    assert.equal(h.events.at(-1), "final-publication");
  }
  assert.equal(invocations, 1);
  assert.equal(h.counters.freed, 1);
  assert.equal(h.counters.terminal, 0);
  assert.equal(h.counters.adopted, 0);
});

for (const cancelDuringAttach of [false, true]) test(`terminal native state transfers the completed source (cancel=${cancelDuringAttach})`, async () => {
  const gate = cancelDuringAttach ? deferred() : null;
  const h = host({ terminalNative: true, demand: true, attachGate: gate });
  let cleanup = 0;
  const done = h.start(async api => {
    await api.awaitSemanticContinuation(h.context);
    cleanup++;
    return h.result();
  });
  await microtasks(); const initial = h.tick(); await h.attach(); await initial; await microtasks();
  const previous = h.endpoint(), continuation = h.run().continuation;
  assert.equal(cleanup, 1); assert.equal(continuation.preparedSourceFinished, true);
  assert.equal(continuation.pending !== null, true);
  assert.equal(h.counters.adopted, 0);
  const attachment = h.attach({ sourceHandoff: true });
  if (cancelDuringAttach) {
    await microtasks(); await h.cancel(); await done;
    assert.equal(h.contexts.size, 1);
    gate.resolve(); await attachment;
    assert.equal(h.contexts.size, 0);
  } else {
    await attachment; await done; previous.stop();
    assert.equal(h.endpoint().completed, true);
    assert.equal(continuation.terminal, true);
    assert.equal(h.errors.length, 0);
    assert.equal(h.events.includes("final-publication"), false, "completed source is attached directly, never returned or driven again");
  }
  assert.equal(cleanup, 1);
  assert.equal(h.counters.freed, 1);
  assert.equal(h.counters.terminal, 1);
  assert.equal(h.counters.adopted, 0);
});

test("initial observed admission relays callbacks to the original stack without creating history", async () => {
  const h = host({ observed: true });
  let invocations = 0, callbacks = 0, statements = 0;
  const done = h.start(async api => {
    invocations++;
    const event = JSON.parse(await api.awaitSemanticContinuation(h.context));
    assert.equal(event.kind, "callback");
    callbacks++;
    const complete = await api.completeContinuationCallback(h.context, JSON.stringify(event.phase.token), "{\"patches\":[]}");
    assert.equal(JSON.parse(complete).kind, "complete");
    statements++;
    assert.equal(JSON.parse(await api.awaitSemanticContinuation(h.context)).kind, "complete");
    statements++;
    return h.result();
  });
  await microtasks();
  const initial = h.tick();
  await h.attach({ initiallyPaused: false, callbackSessionId: 9 });
  assert.equal(h.errors.length, 0);
  const continuation = h.run().continuation;
  const phase = { token: { sequence: 7 } };
  assert.equal(await h.requestContinuationCallback(continuation, phase, {}), '{"patches":[]}');
  await initial;
  h.completeContinuation(continuation, continuation.generation);
  await microtasks();
  const second = h.tick();
  h.completeContinuation(continuation, continuation.generation);
  await second; await done;
  assert.equal(invocations, 1); assert.equal(callbacks, 1); assert.equal(statements, 2);
  assert.equal(h.counters.initial, 0); assert.equal(h.counters.segments, 0);
  assert.equal(h.counters.terminal, 0); assert.equal(h.counters.adopted, 0);
  assert.deepEqual(h.events, ["observe-segment", "final-publication"]);
  assert.equal(h.tasks.size, 0);
});

test("prepared attachment rejects replacement and invalid transport before completing its first segment", async () => {
  const h = host();
  const done = h.start(async api => { await api.awaitSemanticContinuation(h.context); return h.result(); });
  await microtasks();
  await h.attach({ replaceExistingEndpoint: true });
  await h.attach({ pacing: "external_samples" });
  await h.attach({ session: 0x100000000 });
  assert.equal(h.counters.initial, 0);
  assert.equal(h.errors.length, 3);
  await h.cancel(); await done;
});

test("failure in the original observed stack retires its endpoint without freeing a nonexistent reader", async () => {
  const h = host({ observed: true });
  const done = h.start(async api => {
    await api.awaitSemanticContinuation(h.context);
    throw new Error("original callback failed");
  });
  await microtasks(); const initial = h.tick();
  await h.attach({ initiallyPaused: false });
  const continuation = h.run().continuation;
  const callback = h.requestContinuationCallback(continuation, { token: { sequence: 1 } }, {});
  const rejected = assert.rejects(callback, /Python authoring continuation failed/);
  await initial; await done; await rejected;
  assert.equal(h.contexts.size, 0);
  assert.equal(h.counters.freed, 0); assert.equal(h.counters.stopped, 0);
  assert.deepEqual(h.events, ["stop", "retire-source"]);
  assert.match(String(h.errors[0].error), /original callback failed/);
});

test("cancellation during observed attachment stops a late endpoint without reviving its source", async () => {
  const gate = deferred(), h = host({ observed: true, attachGate: gate });
  let resumed = false;
  const done = h.start(async api => { await api.awaitSemanticContinuation(h.context); resumed = true; return h.result(); });
  await microtasks(); const initial = h.tick();
  const attachment = h.attach({ initiallyPaused: false });
  await microtasks(); await h.cancel(); await done;
  gate.resolve(); await attachment; await initial;
  assert.equal(resumed, false); assert.equal(h.contexts.size, 0);
  assert.equal(h.events.filter(event => event === "stop").length, 1);
  assert.equal(h.counters.freed, 0); assert.equal(h.counters.initial, 0);
});

for (const trapAt of ["initial", "reader", "segment", "terminal", "reserve", "adopt", "demand"]) {
  test(`direct prepared ${trapAt} trap retires the worker before any Rust cleanup`, async () => {
    const h = host({ demand: true, trapAt });
    const done = h.start(async api => {
      await api.awaitSemanticContinuation(h.context);
      await api.awaitSemanticContinuation(h.context);
      return h.result();
    });
    await microtasks(); const initial = h.tick(); await h.attach(); await initial;
    await microtasks();
    while (h.tasks.size > 0 && !h.fatal()) { await h.tick(); await microtasks(); }
    await done;
    assert.equal(h.fatal(), true);
    assert.equal(h.errors.length, 1);
    assert.equal(h.errors[0].requestId, null, "trap uses the fatal worker channel");
    assert.ok(h.errors[0].error instanceof WebAssembly.RuntimeError);
    assert.equal(h.counters.stopped, 0, "trap cannot call source stop in the unusable engine");
    assert.equal(h.counters.freed, 0, "trap cannot run WASM destructors");
    assert.equal(h.events.includes("retire-source"), false, "trap cannot re-enter Rust source retirement");
    assert.equal(h.posts.some(post => post.type === "result"), false);
  });
}

function staticResult(h) {
  return JSON.stringify({ kind: "semantic_scene", duration: 0,
    semantic_execution: { context_id: h.registerSemanticExecution(h.context), callback_session_id: null } });
}

for (const zeroWait of [false, true]) {
  test(`completed native-input source keeps original owner after ${zeroWait ? "zero wait" : "no wait"}`, async () => {
    const h = host({ staticNative: true, initialIntervals: [false] }); let invocations = 0;
    const done = h.start(async api => {
      invocations++;
      if (zeroWait) await api.awaitSemanticContinuation(h.context);
      const result = JSON.parse(zeroWait ? h.result() : staticResult(h));
      result.duration = 0; return JSON.stringify(result);
    });
    if (zeroWait) {
      await microtasks(); const attaching = h.attach();
      await h.tick(); await attaching;
    } else {
      await done; await h.attachFinished();
    }
    await done;
    assert.deepEqual(h.errors, []);
    assert.equal(invocations, 1);
    assert.equal(h.counters.initial, Number(zeroWait));
    assert.equal(h.counters.segments + h.counters.terminal + h.counters.adopted, 0);
    assert.equal(h.events.filter(event => event === "static-reader").length, 1);
    h.endpoint().stop();
    assert.equal(h.counters.freed, 0, "no history reader was allocated");
    assert.equal(h.contexts.size, 0);
    assert.equal(h.events.filter(event => event === "retire-source").length, 1);
  });

  test(`completed native-input ${zeroWait ? "cancellation" : "release"} during renderer attachment cannot revive owner`, async () => {
    const gate = deferred(), h = host({ staticNative: true, initialIntervals: [false], attachGate: gate });
    const done = h.start(async api => {
      if (zeroWait) await api.awaitSemanticContinuation(h.context);
      const result = JSON.parse(zeroWait ? h.result() : staticResult(h));
      result.duration = 0; return JSON.stringify(result);
    });
    let attaching;
    if (zeroWait) {
      await microtasks(); attaching = h.attach();
      await h.tick(); await microtasks();
      await h.cancel();
    } else {
      await done; attaching = h.attachFinished();
      await microtasks(); await h.releaseFinished();
    }
    assert.equal(h.contexts.size, 1, "attachment reserves owner until its async setup settles");
    gate.resolve(); await attaching; await done;
    assert.equal(h.contexts.size, 0);
    assert.equal(h.counters.freed, 0);
    assert.equal(h.posts.some(post => post.type === "semantic_execution_attached"), false);
    assert.equal(h.events.filter(event => event === "retire-source").length, 1);
  });
}

test("completed static source attaches the same prepared player once without segment work", async () => {
  const h = host(); let invocations = 0;
  await h.start(async () => { invocations++; return staticResult(h); });
  assert.equal(invocations, 1);
  assert.deepEqual(h.errors, []);
  const result = JSON.parse(h.posts.find(post => post.type === "result").resultJson);
  assert.equal(result.duration, 0);
  assert.equal(result.semantic_execution.prepared_playback, true);
  assert.equal(h.posts.some(post => post.type === "semantic_continuation_registered"), false);
  await h.attachFinished();
  assert.deepEqual(h.errors, []);
  await h.attachFinished();
  assert.match(String(h.errors[0].error), /single-use/);
  assert.equal(h.events.filter(event => event === "static-reader").length, 1);
  assert.equal(h.counters.initial + h.counters.segments + h.counters.terminal, 0);
  h.endpoint().stop();
  assert.equal(h.counters.freed, 1);
  assert.equal(h.contexts.size, 0);
});

test("static source cleanup failure retires its context without publishing a result", async () => {
  const h = host();
  await h.start(async () => { staticResult(h); throw new Error("source cleanup failed"); });
  assert.match(String(h.errors[0].error), /source cleanup failed/);
  assert.equal(h.posts.some(post => post.type === "result"), false);
  assert.equal(h.contexts.size, 0);
  assert.equal(h.events.includes("static-reader"), false);
});

test("released static source cannot revive after asynchronous renderer attachment", async () => {
  const gate = deferred(), h = host({ attachGate: gate });
  await h.start(async () => staticResult(h));
  const attaching = h.attachFinished();
  await microtasks();
  await h.releaseFinished();
  gate.resolve(); await attaching;
  assert.match(String(h.errors.at(-1).error), /retired during attachment|released during attachment/);
  assert.equal(h.counters.freed, 1);
  assert.equal(h.contexts.size, 0);
  assert.equal(h.posts.some(post => post.type === "semantic_execution_attached"), false);
});

test("failed renderer attachment never frees a static reader already retired by its endpoint", async () => {
  const gate = deferred(), h = host({ attachGate: gate });
  await h.start(async () => staticResult(h));
  const attaching = h.attachFinished();
  await microtasks();
  h.endpoint().stop();
  gate.reject(new Error("renderer attachment failed"));
  await attaching;
  assert.match(String(h.errors.at(-1).error), /renderer attachment failed/);
  assert.equal(h.counters.freed, 1);
  assert.equal(h.fatal(), false);
  assert.equal(h.contexts.size, 0);
});

test("fatal source failure after static registration never calls back into trapped WASM", async () => {
  const h = host(), error = new WebAssembly.RuntimeError("source cleanup trap");
  await h.start(async () => {
    staticResult(h);
    h.failAuthoringWorker(error);
    throw error;
  });
  assert.equal(h.fatal(), true);
  assert.equal(h.events.includes("retire-source"), false);
  assert.equal(h.posts.some(post => post.type === "result"), false);
  assert.deepEqual(h.errors.map(item => item.requestId), [null]);
});


test("initial zero barriers yield separately before attaching the first readable prefix", async () => {
  const h = host({ initialIntervals: [false, false, true] }); let statements = 0;
  const done = h.start(async api => {
    for (let i = 0; i < 3; i++) { await api.awaitSemanticContinuation(h.context); statements++; }
    return h.result();
  });
  await microtasks(); const attaching = h.attach();
  for (let i = 1; i <= 2; i++) {
    await h.tick(); await microtasks();
    assert.equal(statements, i); assert.equal(h.counters.initial, i);
    assert.equal(h.events.includes("reader"), false); assert.equal(h.tasks.size, 1);
  }
  const third = h.tick(); await attaching; await third;
  assert.equal(h.counters.initial, 3); assert.equal(statements, 2);
  assert.equal(h.events.filter(event => event === "reader").length, 1);
  h.demand(true); await h.tick(); await done;
  assert.equal(statements, 3); assert.deepEqual(h.errors, []);
  h.endpoint().stop();
});

test("zero-only source attaches a finished reader after cleanup without publishing a suffix", async () => {
  const h = host({ initialIntervals: [false, false] });
  const done = h.start(async api => {
    await api.awaitSemanticContinuation(h.context);
    await api.awaitSemanticContinuation(h.context);
    h.events.push("cleanup");
    const result = JSON.parse(h.result()); result.duration = 0; return JSON.stringify(result);
  });
  await microtasks(); const attaching = h.attach();
  await h.tick(); await microtasks(); await h.tick();
  await attaching; await done;
  assert.ok(h.events.indexOf("cleanup") < h.events.indexOf("static-reader"));
  assert.equal(h.counters.initial, 2); assert.equal(h.counters.terminal + h.counters.adopted, 0);
  assert.deepEqual(h.errors, []); h.endpoint().stop();
});

test("cancellation while initial zero barriers are loading cannot revive an attachment", async () => {
  const h = host({ initialIntervals: [false] }); let statements = 0;
  const done = h.start(async api => {
    await api.awaitSemanticContinuation(h.context); statements++;
    await api.awaitSemanticContinuation(h.context); statements++;
    return h.result();
  });
  await microtasks(); const attaching = h.attach();
  await h.tick(); await microtasks(); await h.cancel(); await attaching; await done;
  assert.equal(statements, 1); assert.equal(h.counters.initial, 1);
  assert.equal(h.events.includes("reader"), false); assert.equal(h.contexts.size, 0);
  assert.equal(h.tasks.size, 0);
});


test("zero-only source cleanup failure rejects pending attachment without a finished reader", async () => {
  const h = host({ initialIntervals: [false] });
  const done = h.start(async api => {
    await api.awaitSemanticContinuation(h.context);
    throw new Error("zero-only cleanup failed");
  });
  await microtasks(); const attaching = h.attach();
  await h.tick(); await attaching; await done;
  assert.equal(h.events.includes("static-reader"), false);
  assert.equal(h.posts.some(post => post.type === "result"), false);
  assert.equal(h.contexts.size, 0);
  assert.equal(h.counters.initial, 1);
});
