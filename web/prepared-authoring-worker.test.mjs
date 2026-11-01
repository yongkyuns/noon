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
  section("function registerContinuationContext", "function continuationEvent"),
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

function host({ demand = false, attachGate = null, completeError = null, trapAt = null,
  preparedPlayback = true, selectedPrepared = false } = {}) {
  const events = [], tasks = new Map(), posts = [], errors = [];
  let taskId = 0, api, execute;
  const counters = { initial: 0, segments: 0, terminal: 0, adopted: 0, reserved: 0, stopped: 0, freed: 0, delivered: 0 };
  let pending, freed = false;
  const alive = () => assert.equal(freed, false, "retired WASM handle must never be reused");
  const trap = operation => { if (operation === trapAt) throw new WebAssembly.RuntimeError(`trap during ${operation}`); };
  const player = {
    prepareSourceSegment(value) {
      alive(); trap("segment"); assert.equal(value, context); counters.segments++; pending = 4; events.push("segment");
    },
    finishSourcePreparation(value) {
      alive(); trap("terminal"); assert.equal(value, context); counters.terminal++; pending = 4; events.push("terminal");
    },
    pendingSourceObjectCapacity() { alive(); trap("capacity"); return pending; },
    reservePrefixObjectCapacity(value) { alive(); trap("reserve"); assert.equal(value, 4); counters.reserved++; },
    deliverSourcePrefix(wallTime) {
      alive(); trap("adopt"); assert.ok(Number.isFinite(wallTime) && wallTime >= 0);
      assert.equal(pending, 4); pending = undefined; counters.adopted++; return true;
    },
    needsPreparation() { alive(); trap("demand"); return demand; },
    stopSourcePreparation() { alive(); counters.stopped++; pending = undefined; },
    free() { alive(); freed = true; counters.freed++; events.push("free"); },
  };
  const context = {
    prepareExecutionRun() {}, liveHandoffDuration: () => 1,
    preparedSourceSelected: () => selectedPrepared,
    retireExecutionSource() { events.push("retire-source"); },
    livePrepareSegment() { counters.initial++; events.push("initial"); trap("initial"); if (completeError) throw completeError; },
    createPreparedExecutionPlayer(session) { trap("reader"); assert.equal(session, 3); events.push("reader"); return player; },
  };
  let endpoint;
  const attachSemanticEngine = async (_context, _request, onStop, ...args) => {
    assert.equal(_context, context);
    assert.deepEqual(args.slice(0, 5), [null, null, null, null, null]);
    assert.equal(args[5].player, player);
    let stopped = false;
    endpoint = {
      sourcePrefixDelivered() { assert.equal(stopped, false); counters.delivered++; args[5].wakeSource(); },
      stop() {
        if (stopped) return;
        stopped = true;
        events.push("stop"); onStop(); player.stopSourcePreparation(); player.free();
      },
    };
    if (attachGate) await attachGate.promise;
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
    return { awaitSemanticContinuation, handleRequest, handleContinuationControl,
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
    start(sourceBody, requestId = 1) {
      execute = sourceBody;
      return api.handleRequest(envelope("run", { requestId, source: "test source", context: {},
        ...(preparedPlayback ? { preparedPlayback: true } : {}), preparationLookaheadSeconds: 0.75 }));
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

test("source delivery samples host time after resource reservation and before adoption", () => {
  let wallTime = 100;
  const events = [];
  const deliver = new Function("invokePreparedEngine", "performance", `
    ${section("function deliverPreparedSourcePrefix", "function createPreparedSourceDriver")}
    return deliverPreparedSourcePrefix;
  `)(callback => callback(), { now() { events.push("sample"); return wallTime; } });
  const continuation = {
    preparedPlayer: {
      pendingSourceObjectCapacity() { events.push("capacity"); return 4; },
      reservePrefixObjectCapacity(capacity) {
        assert.equal(capacity, 4); events.push("reserve"); wallTime = 55_000;
      },
      deliverSourcePrefix(sample) {
        assert.equal(sample, 55_000); events.push("adopt"); return true;
      },
    },
    endpoint: { sourcePrefixDelivered() { events.push("notify"); } },
  };
  assert.equal(deliver(continuation), true);
  assert.deepEqual(events, ["capacity", "reserve", "sample", "adopt", "notify"]);
});

test("normal run adopts canonical soundtrack preparation without restarting source", async () => {
  const h = host({ preparedPlayback: false, selectedPrepared: true });
  let entries = 0, after = 0;
  const done = h.start(async api => {
    entries++;
    await api.awaitSemanticContinuation(h.context);
    after++;
    return h.result();
  });
  await microtasks();
  assert.equal(h.posts[0].semanticExecution.prepared_playback, true);
  assert.equal(entries, 1);
  assert.equal(after, 0);
  const initial = h.tick(); await h.attach(); await initial;
  h.demand(true); await h.tick(); await done;
  assert.equal(entries, 1);
  assert.equal(after, 1);
  assert.equal(h.counters.initial, 1);
  assert.equal(h.counters.terminal, 1);
  assert.equal(h.errors.length, 0);
  h.endpoint().stop();
});

test("ordinary finite source leaves the interpreter preparation driver dormant", async () => {
  const h = host({ preparedPlayback: false });
  let entries = 0;
  await h.start(async () => {
    entries++;
    return JSON.stringify({ kind: "semantic_scene", duration: 3,
      semantic_execution: { context_id: "finite" } });
  });
  assert.equal(entries, 1);
  assert.equal(h.tasks.size, 0);
  assert.equal(h.counters.initial, 0);
  assert.equal(h.counters.terminal, 0);
  assert.equal(h.errors.length, 0);
  assert.equal(JSON.parse(h.posts[0].resultJson).semantic_execution.prepared_playback, undefined);
});

test("ordinary realtime source keeps its existing suspension without preparation scheduling", async () => {
  const h = host({ preparedPlayback: false });
  let entries = 0;
  const done = h.start(async api => {
    entries++;
    await api.awaitSemanticContinuation(h.context);
    throw new Error("cancelled source must not resume");
  });
  await microtasks();
  assert.equal(h.posts[0].semanticExecution.prepared_playback, undefined);
  assert.equal(h.tasks.size, 0);
  assert.equal(h.counters.initial, 0);
  await h.cancel(); await done;
  assert.equal(entries, 1);
  assert.equal(h.errors.filter(error => error.requestId === 1).length, 1);
  assert.equal(h.counters.terminal, 0);
});

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

test("source without a readable barrier fails explicitly", async () => {
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

test("prepared attachment rejects replacement and callbacks before completing its first segment", async () => {
  const h = host();
  const done = h.start(async api => { await api.awaitSemanticContinuation(h.context); return h.result(); });
  await microtasks();
  await h.attach({ replaceExistingEndpoint: true });
  await h.attach({ callbackSessionId: 9 });
  await h.attach({ pacing: "external_samples" });
  await h.attach({ session: 0x100000000 });
  assert.equal(h.counters.initial, 0);
  assert.equal(h.errors.length, 4);
  await h.cancel(); await done;
});

for (const trapAt of ["initial", "reader", "segment", "terminal", "capacity", "reserve", "adopt", "demand"]) {
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
    assert.equal(h.posts.some(post => post.type === "result"), false);
  });
}
