import assert from "node:assert/strict";
import { test } from "node:test";
import { SemanticPreviewSession } from "./semantic-preview-session.js";

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const flush = () => new Promise((resolve) => setImmediate(resolve));

function fixture(t, overrides = {}, limits = {}) {
  const source = deferred();
  const calls = [];
  const registration = { semanticExecution: { contextId: 7, continuationGeneration: 1 } };
  let callbacks;
  let executionOptions;
  const authoring = {
    ready: async () => {},
    run: (_source, _context, options) => {
      callbacks = options;
      queueMicrotask(() => options.onSemanticContinuation(registration).catch(source.reject));
      return source.promise;
    },
    terminate: () => calls.push("authoring:terminate"),
    ...overrides.authoring,
  };
  const execution = {
    prepare: async () => { calls.push("prepare"); },
    startSemanticExecution: async (descriptor, options) => {
      calls.push("start");
      assert.deepEqual(descriptor, registration.semanticExecution);
      assert.equal(options.authoringClient, authoring);
      assert.equal(options.pacing, "external_samples");
      assert.equal(options.transportMode, "transferable");
    },
    sampleToAuthoredTime: async (time) => { calls.push(time); return { time }; },
    metrics: async () => ({ metrics: { backend: "WebGL2", objectCount: 2, drawCalls: 1 } }),
    terminate: () => calls.push("execution:terminate"),
    ...overrides.execution,
  };
  const session = new SemanticPreviewSession({
    createAuthoringClient: () => authoring,
    createExecutionClient: (options) => { executionOptions = options; return execution; },
    ...limits,
  });
  t.after(() => session.close());
  return { session, source, calls, authoring, execution,
    callbacks: () => callbacks, onError: (error) => executionOptions.onError(error) };
}

test("first frame is ready before source completion and unknown duration stays null", async (t) => {
  const { session, calls, source } = fixture(t);
  const ready = await session.open("from noon import *");
  assert.deepEqual(calls, ["prepare", "start", 0]);
  assert.equal(ready.state, "ready");
  assert.equal(ready.sourceState, "running");
  assert.equal(ready.authoredDuration, null);
  assert.equal(ready.frame.publishedTime, 0);
  assert.equal(ready.frame.rendererBackend, "WebGL2");
  source.resolve({ duration: 3 });
  await flush();
  assert.equal(session.snapshot.sourceState, "completed");
  assert.equal(session.snapshot.authoredDuration, 3);
  assert.equal((await session.sample(3)).frame.requestedTime, 3);
});

test("reported actual time is not replaced with requested time", async (t) => {
  const { session } = fixture(t, { execution: { sampleToAuthoredTime: async (time) => ({ time: Math.min(time, 1) }) } });
  await session.open("scene");
  const sample = await session.sample(2);
  assert.equal(sample.frame.requestedTime, 2);
  assert.equal(sample.frame.publishedTime, 1);
});

test("invalid or backward sampling never calls the engine or poisons a valid session", async (t) => {
  const { session, calls } = fixture(t);
  await session.open("scene");
  await session.sample(1);
  for (const time of [-1, NaN, Infinity, "2", 0.5]) {
    await assert.rejects(session.sample(time), /preview/);
  }
  assert.deepEqual(calls, ["prepare", "start", 0, 1]);
  assert.equal((await session.sample(1)).state, "ready");
});

test("source byte, time and sample-count bounds are explicit", async (t) => {
  const { session } = fixture(t, {}, { maxSourceBytes: 4, maxSamples: 2, maxTimeSeconds: 3 });
  await assert.rejects(session.open("🟣x"), /byte limit/);
  await session.open("ok");
  await assert.rejects(session.sample(4), /limit/);
  await session.sample(1);
  await assert.rejects(session.sample(2), /limit/);
  assert.equal(session.snapshot.state, "ready");
});

test("invalid source and loop options do not consume the session", async (t) => {
  const { session, calls } = fixture(t);
  await assert.rejects(session.open(""), /non-empty/);
  await assert.rejects(session.open("ok", { loopDurationSeconds: NaN }), /duration/);
  assert.equal(session.snapshot.state, "new");
  assert.deepEqual(calls, []);
  await session.open("ok");
  await assert.rejects(session.open("again"), /only once/);
});

test("missing continuation is an explicit failure, not a document fallback", async (t) => {
  const { session, calls } = fixture(t, { authoring: { run: async () => ({ duration: 0 }) } });
  await assert.rejects(session.open("scene"), /no semantic continuation/);
  assert.equal(session.snapshot.state, "failed");
  assert.deepEqual(calls, ["authoring:terminate"]);
});

test("source failure before attachment retires the authoring worker", async (t) => {
  const { session, calls } = fixture(t, { authoring: { run: async () => { throw new Error("syntax error"); } } });
  await assert.rejects(session.open("scene"), /syntax error/);
  assert.deepEqual(calls, ["authoring:terminate"]);
});

test("asynchronous source failure preserves the last sampled frame and stops both clients", async (t) => {
  const { session, calls, source } = fixture(t);
  await session.open("scene");
  await session.sample(1);
  source.reject(new Error("callback failed"));
  await flush();
  assert.equal(session.snapshot.state, "failed");
  assert.equal(session.snapshot.frame.publishedTime, 1);
  await assert.rejects(session.sample(2), /callback failed/);
  assert.deepEqual(calls.slice(-2), ["execution:terminate", "authoring:terminate"]);
});

test("malformed source duration is not silently invented", async (t) => {
  const { session, source } = fixture(t);
  await session.open("scene");
  source.resolve({ duration: NaN });
  await flush();
  assert.equal(session.snapshot.state, "failed");
  assert.match(session.snapshot.error, /invalid duration/);
});

test("startup failure cleans the prepared execution owner", async (t) => {
  const { session, calls } = fixture(t, { execution: {
    startSemanticExecution: async () => { throw new Error("GPU initialization failed"); },
  } });
  await assert.rejects(session.open("scene"), /GPU initialization failed/);
  assert.deepEqual(calls, ["prepare", "execution:terminate", "authoring:terminate"]);
});

test("close during blocked preparation rejects immediately and late success cannot revive startup", async (t) => {
  const gate = deferred();
  const entered = deferred();
  const { session, calls } = fixture(t, { execution: {
    prepare: () => { entered.resolve(); return gate.promise; },
  } });
  const opening = session.open("scene");
  await entered.promise;
  session.close("user canceled");
  await assert.rejects(opening, /user canceled/);
  gate.resolve();
  await flush();
  assert.deepEqual(calls, ["execution:terminate", "authoring:terminate"]);
  assert.equal(session.snapshot.state, "closed");
  assert.equal(session.snapshot.sourceState, "canceled");
});

test("close during authoring readiness cannot launch source later", async (t) => {
  const gate = deferred();
  const { session, calls } = fixture(t, { authoring: { ready: () => gate.promise } });
  const opening = session.open("scene");
  session.close();
  await assert.rejects(opening, /closed/);
  gate.resolve();
  await flush();
  assert.deepEqual(calls, ["authoring:terminate"]);
});

test("operation timeout retires a stuck host rather than only abandoning the promise", async (t) => {
  const { session, calls } = fixture(t, { execution: { prepare: () => new Promise(() => {}) } }, { timeoutMs: 20 });
  await assert.rejects(session.open("scene"), /timed out/);
  assert.equal(session.snapshot.state, "failed");
  assert.deepEqual(calls, ["execution:terminate", "authoring:terminate"]);
});

test("concurrent sample is rejected, and close aborts the in-flight sample", async (t) => {
  const { session, execution, calls } = fixture(t);
  await session.open("scene");
  const pending = deferred();
  execution.sampleToAuthoredTime = () => pending.promise;
  const sampling = session.sample(1);
  await assert.rejects(session.sample(2), /already in progress/);
  session.close();
  await assert.rejects(sampling, /closed/);
  pending.resolve({ time: 1 });
  await flush();
  assert.equal(session.snapshot.frame.publishedTime, 0);
  assert.deepEqual(calls.slice(-2), ["execution:terminate", "authoring:terminate"]);
});

test("duplicate continuation fails without allocating another execution owner", async (t) => {
  const { session, callbacks, calls } = fixture(t);
  await session.open("scene");
  await assert.rejects(callbacks().onSemanticContinuation({ semanticExecution: {} }), /multiple/);
  assert.equal(calls.filter((call) => call === "prepare").length, 1);
  assert.equal(session.snapshot.state, "failed");
});

test("out-of-band renderer failure stops the session even between operations", async (t) => {
  const { session, onError } = fixture(t);
  await session.open("scene");
  onError(new Error("device lost"));
  assert.equal(session.snapshot.state, "failed");
  await assert.rejects(session.sample(1), /device lost/);
});

test("missing published time or backend metadata fails rather than reporting a fake frame", async (t) => {
  for (const execution of [
    { sampleToAuthoredTime: async () => ({}) },
    { metrics: async () => ({ metrics: {} }) },
  ]) {
    const { session } = fixture(t, { execution });
    await assert.rejects(session.open("scene"), /invalid sample metadata/);
    assert.equal(session.snapshot.frame, null);
  }
});

test("cleanup remains idempotent and tries both clients if one terminator fails", async (t) => {
  const { session, calls } = fixture(t, { execution: { terminate: () => { calls.push("throwing cleanup"); throw new Error("cleanup failed"); } } });
  await session.open("scene");
  session.close();
  session.close();
  assert.deepEqual(calls.slice(-2), ["throwing cleanup", "authoring:terminate"]);
  assert.equal(session.snapshot.cleanupErrors.length, 1);
});

test("synchronous factory error notification cannot leak a just-returned owner", async (t) => {
  const { session, calls } = fixture(t, {}, {
    createExecutionClient: ({ onError }) => {
      onError(new Error("construction failed"));
      return { terminate: () => calls.push("late-owner:terminate") };
    },
  });
  await assert.rejects(session.open("scene"), /construction failed/);
  assert.deepEqual(calls, ["authoring:terminate", "late-owner:terminate"]);
});

test("snapshots cannot mutate internal metadata, and completed source survives close", async (t) => {
  const { session, source } = fixture(t);
  await session.open("scene");
  const snapshot = session.snapshot;
  snapshot.frame.publishedTime = 99;
  snapshot.cleanupErrors.push("injected");
  assert.equal(session.snapshot.frame.publishedTime, 0);
  assert.equal(session.snapshot.cleanupErrors.length, 0);
  source.resolve({ duration: 0 });
  await flush();
  session.close();
  assert.equal(session.snapshot.sourceState, "completed");
  assert.equal(session.snapshot.capabilities.forwardSampling, false);
});

test("independent runs do not cancel or mutate each other's owners", async (t) => {
  const first = fixture(t);
  const second = fixture(t);
  await Promise.all([first.session.open("one"), second.session.open("two")]);
  first.session.close();
  assert.equal((await second.session.sample(2)).frame.publishedTime, 2);
  assert.equal(second.calls.includes("authoring:terminate"), false);
});

test("invalid limits are rejected without creating a host", () => {
  for (const limits of [{ timeoutMs: 0 }, { maxSamples: 1.5 }, { maxSourceBytes: Infinity }, { maxTimeSeconds: -1 }]) {
    assert.throws(() => new SemanticPreviewSession({ createAuthoringClient() {}, createExecutionClient() {}, ...limits }), RangeError);
  }
});
