import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import { createPreparedInterpreterDriver } from "./prepared-interpreter-driver.js";

// Execute the production worker orchestration and real suspension driver. Only
// the Python invocation, typed Rust endpoint and worker messaging are simulated;
// these tests do not establish Rust accounting, Pyodide or GPU correctness.
const source = readFileSync(new URL("./python-worker.source.js", import.meta.url), "utf8");
function section(start, end) {
  const first = source.indexOf(start), last = source.indexOf(end, first + start.length);
  assert.ok(first >= 0 && last > first, `production worker boundary: ${start}`);
  assert.equal(source.indexOf(start, first + start.length), -1, `unique boundary: ${start}`);
  return source.slice(first, last);
}
const production = [
  section("function failAuthoringWorker(error)", "// A WASM trap"),
  section("function invokePreparedEngine(callback)", "function continuationEvent("),
  section("function failContinuation(continuation, error)", "function isContinuationControl("),
  section("async function handleRequest(request)", "async function attachSemanticExecutionRequest("),
].join("\n");
const flush = () => new Promise(resolve => setImmediate(resolve));

function fixture({ failureAt = null, failure = "budget rejected", catchBarrier = false, quiet = false } = {}) {
  const tasks = new Map(), calls = [], messages = [], errors = [], runs = [];
  const semanticContexts = new Map();
  let nextTask = 0, nextRun = 0, armed = true, closed = 0;
  const attempt = (stage, run) => {
    calls.push([stage, run]);
    if (armed && failureAt === stage) throw failure;
  };
  const context = vm.createContext({
    semanticContexts, activeAuthoringRun: null, fatalAuthoringFailure: false,
    pyodidePromise: Promise.resolve({}), performance, WebAssembly,
    validateRequest() {},
    post: (type, payload) => messages.push({ type, ...payload }),
    postError: (requestId, error) => errors.push({ requestId, error }),
    self: { close() { closed++; } },
    retireSemanticContext() { assert.fail("an attached historical reader must not be retired"); },
    createPreparedInterpreterDriver: options => createPreparedInterpreterDriver({
      ...options,
      setTimeout(callback) { tasks.set(++nextTask, callback); return nextTask; },
      clearTimeout(handle) { tasks.delete(handle); },
    }),
    async runAuthoringSource() {
      const owner = context.activeAuthoringRun;
      const id = ++nextRun;
      const history = ["initial readable prefix"];
      const frame = Object.freeze({ time: 0.25, paused: true });
      const record = {
        id, owner, history, frame, pending: null, sourceOpen: true, finished: false,
        statements: 0, cleanup: 0, caught: null, stopped: 0, notifications: 0,
      };
      runs.push(record);
      const player = {
        prepareSourceSegment(scene) {
          assert.equal(scene, continuation.context);
          attempt("prepare", id);
          record.pending = quiet ? null : { terminal: false };
        },
        pendingSourceObjectCapacity() { return record.pending === null ? undefined : 2; },
        reservePrefixObjectCapacity(capacity) {
          assert.equal(capacity, 2);
          attempt(record.pending.terminal ? "terminal-reserve" : "reserve", id);
        },
        deliverSourcePrefix() {
          attempt(record.pending.terminal ? "terminal-deliver" : "deliver", id);
          if (record.pending.terminal) record.finished = true;
          else history.push("new readable prefix");
          record.pending = null;
          return true;
        },
        finishSourcePreparation(scene) {
          assert.equal(scene, continuation.context);
          attempt("finish", id);
          record.pending = { terminal: true };
        },
        needsPreparation() { attempt("demand", id); return true; },
        stopSourcePreparation() {
          attempt("stop-source", id);
          record.sourceOpen = false;
          record.pending = null;
          record.stopped++;
        },
      };
      const endpoint = {
        sourcePrefixDelivered() { record.notifications++; },
        stop() { assert.fail("ordinary preparation rejection must not stop the reader"); },
        publishContinuationResult() { assert.fail("prepared history must not reconcile from source"); },
      };
      const continuation = {
        contextId: `context-${id}`, generation: id, context: {}, terminal: false,
        preparedPlayback: true, preparedPlayer: player, preparedDriver: owner.preparedDriver,
        preparedReady: { promise: Promise.resolve(), reject(error) { record.readyRejection = error; } },
        preparedBootstrapObserved: false, preparedAttachmentStarted: true,
        endpoint, callbackCommit: null, callbackRequest: null, pending: null,
      };
      record.continuation = continuation;
      record.player = player;
      record.endpoint = endpoint;
      record.entry = { released: false, context: continuation.context };
      semanticContexts.set(continuation.contextId, record.entry);
      owner.continuation = continuation;
      try {
        record.statements++;
        await owner.preparedDriver.awaitSegment(continuation.context);
        record.statements++;
        try {
          await owner.preparedDriver.awaitSegment(continuation.context);
        } catch (error) {
          if (!catchBarrier) throw error;
          record.caught = error;
        }
        record.statements++;
        return JSON.stringify({ semantic_execution: {
          context_id: continuation.contextId, continuation_generation: continuation.generation,
        } });
      } finally { record.cleanup++; }
    },
  });
  vm.runInContext(`${production}\nglobalThis.testHandleRequest = handleRequest;`, context);
  return {
    tasks, calls, messages, errors, runs, context,
    get closed() { return closed; },
    disarm() { armed = false; },
    async run(requestId = 1) {
      const done = context.testHandleRequest({ type: "run", requestId, preparedPlayback: true });
      await flush();
      return { done, record: runs.at(-1) };
    },
    async drain() {
      // A bounded deterministic task queue, not an elapsed-time poll or retry.
      for (let turns = 0; tasks.size !== 0; turns++) {
        assert.ok(turns < 8, "worker must stop or finish without an automatic retry loop");
        assert.equal(tasks.size, 1, "one suspended-source preparation task");
        const [id, callback] = tasks.entries().next().value;
        tasks.delete(id);
        await callback();
        await flush();
      }
    },
  };
}

async function assertOrdinaryFailure(f, done, record, expected) {
  await f.drain();
  await done;
  assert.deepEqual(f.errors, [{ requestId: 1, error: expected }], "preserve the engine rejection exactly");
  assert.deepEqual(f.messages, [], "failure must not publish a success descriptor");
  assert.equal(f.closed, 0, "a budget rejection is not an interpreter-fatal trap");
  assert.equal(f.context.fatalAuthoringFailure, false);
  assert.equal(f.context.activeAuthoringRun, null, "failed source invocation must be released");
  assert.equal(record.owner.preparedDriver.state(), "failed");
  assert.equal(record.cleanup, 1);
  assert.equal(record.stopped, 1, "retire unpublished source output exactly once");
  assert.equal(record.sourceOpen, false);
  assert.equal(record.pending, null);
  assert.equal(record.finished, false, "rejection cannot invent a final endpoint");
  assert.equal(record.continuation.terminal, true, "source invocation is terminal, not reader history");
  assert.equal(record.continuation.preparedPlayer, record.player);
  assert.equal(record.continuation.endpoint, record.endpoint);
  assert.equal(f.context.semanticContexts.get(record.continuation.contextId), record.entry);
  assert.equal(record.entry.released, false, "keep adopted history owned by the same context");
  assert.deepEqual(record.frame, { time: 0.25, paused: true });
  const before = f.calls.length;
  for (let i = 0; i < 10; i++) record.owner.preparedDriver.wake();
  await flush();
  assert.equal(f.tasks.size, 0);
  assert.equal(f.calls.length, before, "playback wakes must not retry rejected source work");
  await assert.rejects(record.owner.preparedDriver.run(() => assert.fail("no source retry")), /single-use/);
}

for (const [stage, label] of [
  ["prepare", "ResourceBudgetExceeded"],
  ["prepare", "HistoryPayloadBudgetExceeded"],
  ["prepare", "ObjectStorageBudgetExceeded"],
  ["reserve", "ObjectStorageBudgetExceeded"],
  ["deliver", "HistoryPayloadBudgetExceeded"],
  ["finish", "ResourceBudgetExceeded"],
  ["finish", "HistoryPayloadBudgetExceeded"],
  ["terminal-reserve", "ObjectStorageBudgetExceeded"],
  ["terminal-deliver", "HistoryPayloadBudgetExceeded"],
]) {
  test(`${label} at ${stage} preserves the reader and stops only the source`, async () => {
    // Rust/WASM error text is deliberately opaque to the host. No JS budget policy.
    const failure = `prepared prefix unavailable: ${label} { limit_bytes: 7 }`;
    const f = fixture({ failureAt: stage, failure });
    const { done, record } = await f.run();
    await assertOrdinaryFailure(f, done, record, failure);
    const terminal = stage === "finish" || stage.startsWith("terminal-");
    assert.deepEqual(record.history, terminal
      ? ["initial readable prefix", "new readable prefix"] : ["initial readable prefix"]);
    assert.equal(record.notifications, terminal ? 1 : 0, "never notify for a rejected adoption");
    assert.equal(record.statements, terminal ? 3 : 2);
    assert.equal(f.calls.filter(([name]) => name === stage).length, 1);
    const sequence = f.calls.map(([name]) => name).filter(name => name !== "demand");
    const order = ["prepare", "reserve", "deliver", "finish", "terminal-reserve", "terminal-deliver"];
    assert.deepEqual(sequence, [...order.slice(0, order.indexOf(stage) + 1), "stop-source"]);
  });
}

test("catching a budget rejection in source cannot seal or report success", async () => {
  const failure = Object.freeze({ category: "opaque engine rejection" });
  const f = fixture({ failureAt: "reserve", failure, catchBarrier: true });
  const { done, record } = await f.run();
  await assertOrdinaryFailure(f, done, record, failure);
  assert.equal(record.caught, failure);
  assert.equal(record.statements, 3, "source caught the failure but did not heal the invocation");
  assert.equal(f.calls.some(([stage]) => stage === "finish"), false);
});

test("demand failure after bootstrap is not retried or mistaken for terminal delivery", async () => {
  const failure = new Error("reader demand unavailable");
  const f = fixture({ failureAt: "demand", failure });
  const { done, record } = await f.run();
  await assertOrdinaryFailure(f, done, record, failure);
  assert.equal(record.statements, 1);
  assert.deepEqual(f.calls.map(([stage]) => stage), ["demand", "stop-source"]);
  assert.deepEqual(record.history, ["initial readable prefix"]);
});

test("a fresh Run succeeds after a budget failure without reusing or retiring the old reader", async () => {
  const failure = "object capacity budget exhausted";
  const f = fixture({ failureAt: "reserve", failure });
  const first = await f.run();
  await assertOrdinaryFailure(f, first.done, first.record, failure);
  f.disarm();
  const second = await f.run(2);
  await f.drain(); await second.done;
  assert.notEqual(first.record.owner.preparedDriver, second.record.owner.preparedDriver);
  assert.notEqual(first.record.player, second.record.player);
  assert.equal(second.record.owner.preparedDriver.state(), "finished");
  assert.equal(second.record.finished, true);
  assert.equal(second.record.stopped, 0);
  assert.equal(second.record.cleanup, 1);
  assert.equal(first.record.stopped, 1);
  assert.equal(first.record.entry.released, false);
  assert.equal(first.record.finished, false);
  assert.equal(f.errors.length, 1);
  assert.equal(f.messages.length, 1);
  assert.equal(f.messages[0].requestId, 2);
  assert.equal(JSON.parse(f.messages[0].resultJson).semantic_execution.prepared_playback, true);
  assert.equal(f.context.activeAuthoringRun, null);
});

test("successful ordinary and terminal adoption notify only after each Rust delivery", async () => {
  const f = fixture();
  const { done, record } = await f.run();
  await f.drain(); await done;
  assert.deepEqual(f.errors, []);
  assert.equal(record.owner.preparedDriver.state(), "finished");
  assert.equal(record.finished, true);
  assert.equal(record.cleanup, 1);
  assert.equal(record.stopped, 0);
  assert.equal(record.notifications, 2);
  assert.deepEqual(f.calls.map(([stage]) => stage), [
    "demand", "prepare", "reserve", "deliver", "demand", "finish", "terminal-reserve", "terminal-deliver",
  ]);
  assert.equal(f.messages.length, 1);
  assert.equal(f.messages[0].type, "result");
  assert.equal(JSON.parse(f.messages[0].resultJson).semantic_execution.prepared_playback, true);
});

test("a zero-duration segment does not reserve or adopt a nonexistent suffix", async () => {
  const f = fixture({ quiet: true });
  const { done, record } = await f.run();
  await f.drain(); await done;
  assert.deepEqual(f.errors, []);
  assert.deepEqual(f.calls.map(([stage]) => stage), [
    "demand", "prepare", "demand", "finish", "terminal-reserve", "terminal-deliver",
  ]);
  assert.equal(record.notifications, 1);
  assert.equal(record.finished, true);
  assert.deepEqual(record.history, ["initial readable prefix"]);
});

test("a WASM trap remains fatal and never calls source cleanup on the trapped engine", async () => {
  const failure = new WebAssembly.RuntimeError("unreachable");
  const f = fixture({ failureAt: "reserve", failure });
  const { done, record } = await f.run();
  await f.drain(); await done;
  assert.equal(f.context.fatalAuthoringFailure, true);
  assert.equal(f.closed, 1);
  assert.equal(f.errors[0].requestId, null, "use the existing worker-fatal channel");
  assert.equal(f.errors[0].error, failure);
  assert.equal(f.messages.length, 0);
  assert.equal(record.stopped, 0, "do not reenter a trapped WASM instance");
  assert.equal(record.cleanup, 1);
  assert.equal(f.context.activeAuthoringRun, null);
});
