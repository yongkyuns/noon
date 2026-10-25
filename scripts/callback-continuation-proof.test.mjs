import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("./callback-continuation-proof.mjs", import.meta.url), "utf8");
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const flush = async () => { for (let i = 0; i < 40; i += 1) await Promise.resolve(); };

function fixture(options = {}) {
  const startGate = deferred();
  const authored = deferred();
  const pending = new Promise(() => {});
  const calls = [];
  const timers = new Map();
  let timerId = 0, time = 0, startup = false, terminated = 0, removed = 0, hooks;
  const registration = { generation: 7, duration: 0.25, semanticExecution: { contextId: "source-1" } };
  const endpoint = options.endpoint ?? 1.5;
  const result = { duration: endpoint, semanticExecution: {
    contextId: "source-1", continuationGeneration: 7, callbackSessionId: 9,
    ...options.resultFields,
  } };
  let errorCallback;
  class Client {
    constructor(_, callbacks) { errorCallback = callbacks.onError; calls.push("construct"); }
    async startSemanticExecution(_, config) {
      calls.push(["start", config]);
      if (options.startFailure) throw new Error("attachment failed");
      if (options.delayStart) await startGate.promise;
      startup = true;
    }
    async sampleToAuthoredTime(value) {
      assert.equal(startup, true, "sample occurred before attachment");
      calls.push(["sample", value]);
      if (options.sampleFailure) throw new Error("callback sample failed");
      if (options.hangSample) return pending;
      time = value;
      if (options.completeEarly || value === endpoint) authored.resolve(result);
      return { time: value };
    }
    async state() {
      assert.equal(startup, true, "query occurred before attachment");
      calls.push(["state", time]);
      if (options.stateFailure) throw new Error("state query failed");
      return { time: options.wrongTime ? time + 1 : time };
    }
    async metrics() { calls.push("metrics"); return { metrics: { time, objectCount: 2, drawCalls: 1 } }; }
    terminate() { terminated += 1; calls.push("terminate"); }
  }
  const harness = { AuthoringExecutionClient: Client, authoring: {
    run(text, _, callbacks) {
      calls.push(["run", text]);
      hooks = callbacks;
      if (options.sourceFailure) return Promise.reject(new Error("source assertion failed"));
      if (options.noRegistration) { authored.resolve(result); return authored.promise; }
      void callbacks.onSemanticContinuation(registration).then(async () => {
        if (options.duplicate) await callbacks.onSemanticContinuation(registration);
        if (options.autonomous) { time = endpoint; authored.resolve(result); }
      }).catch(authored.reject);
      return authored.promise;
    },
  } };
  const context = vm.createContext({
    window: { sharedAuthoringSmoke: harness },
    document: {
      createElement() { return { remove() { removed += 1; } }; },
      body: { append() {} },
    },
    setTimeout(fn, delay) { const id = ++timerId; timers.set(id, { fn, delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
  });
  vm.runInContext(source.replaceAll("export ", ""), context);
  return {
    context, calls, timers, startGate, authored, harness,
    start: (more = {}) => context.startCallbackProof({ source: "unchanged Python fixture", canvasId: "proof", pacing: "external_samples", sampleTime: 0.75, ...more }),
    finish: () => context.finishCallbackProof(endpoint),
    stop: () => context.disposeCallbackProof(),
    error: () => errorCallback("fatal renderer failure", "render"),
    counts: () => ({ terminated, removed }),
    expire() {
      assert.equal(timers.size, 1, "only one timeout, never a polling loop");
      const [id, timer] = timers.entries().next().value;
      assert.equal(timer.delay, 30_000);
      timers.delete(id);
      timer.fn();
    },
    lateRegister: () => hooks.onSemanticContinuation(registration),
  };
}

const samples = f => f.calls.filter(x => Array.isArray(x) && x[0] === "sample").map(x => x[1]);
const runCount = f => f.calls.filter(x => Array.isArray(x) && x[0] === "run").length;

test("exact samples follow completed attachment and preserve a suspended source", async () => {
  const f = fixture();
  const value = await f.start();
  assert.equal(value.midpoint.time, 0.75);
  assert.equal(f.harness.callbackProof.settled, false);
  assert.deepEqual(samples(f), [0, 0.75]);
  assert.equal(f.calls.find(x => Array.isArray(x) && x[0] === "start")[1].pacing, "external_samples");
  assert.equal(runCount(f), 1);
  assert.equal(f.timers.size, 0);
  f.stop();
});

test("arbitrarily delayed attachment consumes no midpoint polling attempts", async () => {
  const f = fixture({ delayStart: true });
  const value = f.start();
  await flush();
  assert.deepEqual(samples(f), []);
  assert.equal(f.calls.filter(x => Array.isArray(x) && x[0] === "state").length, 0);
  assert.equal(f.timers.size, 1);
  f.startGate.resolve();
  assert.equal((await value).midpoint.time, 0.75);
  f.stop();
});

test("endpoint is sampled before awaiting the suspended source result", async () => {
  const f = fixture();
  await f.start();
  assert.equal((await f.finish()).authored.duration, 1.5);
  assert.deepEqual(samples(f), [0, 0.75, 1.5]);
  assert.equal(runCount(f), 1);
  f.stop();
});

test("realtime proof completes autonomously and sends no samples", async () => {
  const f = fixture({ autonomous: true });
  await f.start({ pacing: "realtime", sampleTime: null });
  assert.equal((await f.finish()).metrics.time, 1.5);
  assert.deepEqual(samples(f), []);
  assert.equal(runCount(f), 1);
  assert.equal(f.calls.find(x => Array.isArray(x) && x[0] === "start")[1].pacing, "realtime");
  f.stop();
});

for (const [flag, message] of [
  ["sourceFailure", "source assertion failed"],
  ["startFailure", "attachment failed"],
  ["sampleFailure", "callback sample failed"],
  ["stateFailure", "state query failed"],
  ["wrongTime", "expected authored time"],
  ["completeEarly", "source completed before midpoint"],
  ["duplicate", "duplicate semantic context"],
  ["noRegistration", "source did not register a continuation"],
]) {
  test(`${flag} rejects without retrying or hiding the failure`, async () => {
    const f = fixture({ [flag]: true });
    await assert.rejects(f.start(), new RegExp(message));
    assert.equal(runCount(f), 1);
    assert.equal(f.harness.callbackProof, null);
    assert.equal(f.timers.size, 0);
    assert.equal(f.counts().removed, 1);
  });
}

test("renderer failure interrupts an otherwise hung sample with its original detail", async () => {
  const f = fixture({ hangSample: true });
  const result = f.start();
  await flush();
  f.error();
  await assert.rejects(result, /render: fatal renderer failure/);
  assert.equal(f.counts().terminated, 1);
});

test("timeout retires the candidate and late readiness cannot reactivate it", async () => {
  const f = fixture({ delayStart: true });
  const result = f.start();
  await flush();
  f.expire();
  await assert.rejects(result, /timed out during attachment/);
  f.startGate.resolve();
  await flush();
  assert.equal(f.harness.callbackProof, null);
  assert.deepEqual(samples(f), []);
  assert.equal(f.counts().terminated, 1);
});

test("hung realtime completion fails with the stage, rather than being retried", async () => {
  const f = fixture();
  await f.start({ pacing: "realtime", sampleTime: null });
  const result = f.finish();
  await flush();
  f.expire();
  await assert.rejects(result, /timed out during realtime source completion/);
  assert.equal(runCount(f), 1);
  assert.deepEqual(samples(f), []);
  assert.equal(f.counts().terminated, 1);
});

for (const [name, value] of [["contextId", "wrong"], ["continuationGeneration", 8], ["callbackSessionId", null]]) {
  test(`completion rejects a changed or invalid ${name}`, async () => {
    const f = fixture({ resultFields: { [name]: value } });
    await f.start();
    await assert.rejects(f.finish(), /callback completion changed/);
    assert.equal(f.harness.callbackProof, null);
  });
}

test("disposal is idempotent and never turns a late registration into another run", async () => {
  const f = fixture();
  await f.start();
  f.stop();
  f.stop();
  await assert.rejects(f.lateRegister(), /late registration/);
  assert.equal(f.counts().terminated, 1);
  assert.equal(f.counts().removed, 1);
  assert.equal(runCount(f), 1);
});

test("invalid pacing rejects before allocating or executing source", async () => {
  const f = fixture();
  await assert.rejects(f.start({ pacing: "realtime" }), /invalid callback proof/);
  assert.equal(runCount(f), 0);
  assert.equal(f.harness.callbackProof, undefined);
});


test("affine fixture uses its own half-second sample and one-second endpoint", async () => {
  const f = fixture({ endpoint: 1 });
  assert.equal((await f.start({ sampleTime: 0.5 })).midpoint.time, 0.5);
  assert.equal((await f.finish()).authored.duration, 1);
  assert.deepEqual(samples(f), [0, 0.5, 1]);
  f.stop();
});

test("renderer error between completion and final screenshot cannot be silently discarded", async () => {
  const f = fixture();
  await f.start();
  await f.finish();
  f.error();
  assert.throws(() => f.stop(), /render: fatal renderer failure/);
  assert.equal(f.harness.callbackProof, null);
  assert.equal(f.counts().terminated, 1);
});
