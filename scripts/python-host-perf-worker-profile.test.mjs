import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { collectWorkerProfile, selectAuthoringTarget, workerInspectorSession,
  validateWorkerProfile } from "./python-host-perf-worker-profile.mjs";

const profile = () => ({ startTime: 1, endTime: 1001,
  nodes: [{ id: 1, callFrame: { functionName: "(root)" } }], samples: [1], timeDeltas: [1000] });
function fixture(failMethod = null, failure = new Error("diagnostic failure")) {
  const calls = [], records = [], observation = { mode: "jspi", center: [1, 0] };
  const session = { target: { targetId: "worker" }, async send(method, params) {
    calls.push([method, params]);
    if (method === failMethod) throw failure;
    if (method === "Profiler.stop") return { profile: profile() };
    if (method === "Runtime.getIsolateId") return { id: "isolate" };
    if (method === "Runtime.getHeapUsage") return { usedSize: 100, totalSize: 200 };
    return {};
  }, async close() { calls.push(["close"]); } };
  return { calls, records, observation, session, arguments: {
    open: async () => session, run: async () => { calls.push(["run"]); return observation; },
    record: async value => records.push(structuredClone(value)),
  } };
}

test("worker selection cannot attach to the other build or renderer", () => {
  const worker = { type: "worker", targetId: "correct", browserContextId: "a", url: "https://a/python-worker.js" };
  const targets = [worker, { ...worker, targetId: "other", browserContextId: "b" },
    { ...worker, targetId: "renderer", url: "https://a/render-worker.js" }, { ...worker, type: "page" }];
  assert.equal(selectAuthoringTarget(targets, "a", worker.url), worker);
  assert.throws(() => selectAuthoringTarget(targets, "missing", worker.url), /exactly one/);
  assert.throws(() => selectAuthoringTarget([...targets, worker], "a", worker.url), /exactly one/);
  assert.throws(() => selectAuthoringTarget(targets, null, worker.url), /owning browser context/);
});

test("CPU sampling surrounds unchanged source; raw profile/heap evidence remain diagnostic", async () => {
  const f = fixture(); const r = await collectWorkerProfile(f.arguments);
  assert.deepEqual(f.calls.map(([name]) => name), ["Runtime.getIsolateId", "Runtime.getHeapUsage",
    "Profiler.enable", "Profiler.setSamplingInterval", "Profiler.start", "run", "Profiler.stop",
    "Profiler.disable", "Runtime.getHeapUsage", "close"]);
  assert.deepEqual(f.calls[3][1], { interval: 1000 });
  assert.equal(f.records[0].complete, false); assert.equal(r.complete, true);
  assert.equal(r.diagnosticOnly, true); assert.equal(r.scope, "whole_source_worker_cpu");
  assert.deepEqual(r.profile, profile()); assert.deepEqual(r.observation, f.observation);
  assert.deepEqual(r.cleanupErrors, []);
});

test("original source failure survives a profiler stop failure and still detaches", async () => {
  const f = fixture("Profiler.stop"); const original = new Error("original source failure");
  f.arguments.run = async () => { throw original; };
  await assert.rejects(collectWorkerProfile(f.arguments), error => error === original);
  const r = f.records.at(-1);
  assert.equal(r.complete, false); assert.match(r.error, /original source/);
  assert.deepEqual(r.cleanupErrors, ["Error: diagnostic failure"]);
  assert.equal(f.calls.at(-1)[0], "close");
  assert.ok(f.calls.some(([name]) => name === "Profiler.disable"));
});

for (const method of ["Runtime.getIsolateId", "Runtime.getHeapUsage", "Profiler.enable",
  "Profiler.setSamplingInterval", "Profiler.start"]) {
  test(`failed ${method} cannot invoke source or leave an inspector attached`, async () => {
    const failure = new Error(method); const f = fixture(method, failure);
    await assert.rejects(collectWorkerProfile(f.arguments), error => error === failure);
    assert.ok(!f.calls.some(([name]) => name === "run"));
    assert.equal(f.calls.at(-1)[0], "close");
    assert.equal(f.records.at(-1).complete, false);
  });
}

test("empty, malformed and stale CPU samples cannot be presented as a valid profile", () => {
  assert.equal(validateWorkerProfile(profile()).samples.length, 1);
  for (const mutate of [p => { p.samples = []; }, p => { p.samples = [3]; },
    p => { p.timeDeltas = []; }, p => { p.timeDeltas = [-1]; },
    p => { p.nodes.push(p.nodes[0]); }, p => { p.endTime = 0; }]) {
    const p = profile(); mutate(p); assert.throws(() => validateWorkerProfile(p));
  }
});

test("nested CDP replies are correlated only within their attached worker", async () => {
  const cdp = new EventEmitter(), requests = [];
  cdp.send = async (method, params) => { requests.push({ method, params }); };
  const session = workerInspectorSession(cdp, "worker");
  try {
    const first = session.send("Runtime.getHeapUsage"), second = session.send("Runtime.getIsolateId");
    await Promise.resolve();
    const reply = (sessionId, id, result) => cdp.emit("Target.receivedMessageFromTarget", {
      sessionId, message: JSON.stringify({ id, result }),
    });
    reply("other-worker", 1, { wrong: true }); reply("worker", 2, { id: "isolate" });
    reply("worker", 1, { usedSize: 10 });
    assert.deepEqual(await first, { usedSize: 10 }); assert.deepEqual(await second, { id: "isolate" });
    assert.deepEqual(requests.map(r => r.method), ["Target.sendMessageToTarget", "Target.sendMessageToTarget"]);
    for (const method of ["Runtime.evaluate", "HeapProfiler.collectGarbage", "Profiler.startPreciseCoverage"])
      assert.throws(() => session.send(method), /only sampling/);
  } finally { await session.close(); }
  assert.equal(cdp.listenerCount("Target.receivedMessageFromTarget"), 0);
  assert.equal(cdp.listenerCount("Target.detachedFromTarget"), 0);
  assert.equal(requests.at(-1).method, "Target.detachFromTarget");
  assert.throws(() => session.send("Profiler.start"), /closed/);
});

for (const mode of ["send", "remote", "detached", "malformed", "null", "timeout"]) {
  test(`inspector ${mode} failure settles the request without leaking listeners`, async () => {
    const cdp = new EventEmitter(); cdp.send = async method => {
      if (mode === "send" && method === "Target.sendMessageToTarget") throw new Error("send failed");
    };
    const session = workerInspectorSession(cdp, "worker", 5);
    const result = session.send("Profiler.start");
    const rejected = assert.rejects(result);
    if (mode === "remote") cdp.emit("Target.receivedMessageFromTarget", {
      sessionId: "worker", message: JSON.stringify({ id: 1, error: { message: "remote failed" } }),
    });
    if (mode === "malformed") cdp.emit("Target.receivedMessageFromTarget", { sessionId: "worker", message: "{" });
    if (mode === "null") cdp.emit("Target.receivedMessageFromTarget", { sessionId: "worker", message: "null" });
    if (mode === "detached") cdp.emit("Target.detachedFromTarget", { sessionId: "worker" });
    await rejected; await session.close();
    assert.equal(cdp.listenerCount("Target.receivedMessageFromTarget"), 0);
  });
}

test("worker profiling cannot alter the history inputs, pairs or budgets", async () => {
  const source = await readFile(new URL("./python-host-perf-history.mjs", import.meta.url), "utf8");
  const start = source.indexOf("    evidence.workerProfiles = {}");
  assert.ok(start > source.indexOf("evidence.complete = true"));
  assert.ok(start > source.indexOf("evidence.costs ="));
  const block = source.slice(start, source.indexOf("    return evidence", start));
  assert.match(block, /measure\(participant, source\)/);
  assert.match(block, /workerProfiles\[name\] = profile/);
  assert.doesNotMatch(block, /pairs\.push|warmups\.push|pairedCost|performanceSource|evidence\.costs\s*=/);
});
