import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { diagnoseWorkerHistory, diagnoseControlledHistory } from "./python-host-perf-history.mjs";
import { PERF_PROTOCOL, performanceSource } from "./python-host-perf-protocol.mjs";

function fixture() {
  const observations = [], records = [], lifecycle = [];
  const retained = { name: "retained" };
  const fresh = { name: "fresh", close: async () => { lifecycle.push("close"); } };
  return { observations, records, lifecycle, retained, fresh,
    arguments: {
      // The collector has its own real-CDP and transport tests. No browser is
      // constructed by these history-order unit tests.
      sampleWorker: async ({ record }) => record({ diagnosticOnly: true, complete: true }),
      identity: { source: "a".repeat(40) }, retained,
      openFresh: async () => { lifecycle.push("open"); return fresh; },
      record: async value => { records.push(structuredClone(value)); },
      measure: async (participant, source) => {
        assert.equal(source, performanceSource("jspi", "deterministic"));
        observations.push(participant.name);
        // Synthetic timing exercises the diagnostic plumbing, not a speed claim.
        return { mode: "jspi", workload: "deterministic", coroutine: false,
          callback_calls: 0, center: [1.2048, 0], metrics: { presentedFrames: 50, objectCount: 600 },
          creation_ms: 10, local_ms: participant === fresh ? 5 : 10, execution_ms: 20 };
      },
    },
  };
}

test("history control retains every prescribed pair and uses unchanged actual-JSPI source", async () => {
  const f = fixture();
  const r = await diagnoseWorkerHistory(f.arguments);
  const order = Array.from({ length: PERF_PROTOCOL.warmups }, (_, i) =>
    i % 2 ? ["fresh", "retained"] : ["retained", "fresh"]);
  for (let i = 0; i < PERF_PROTOCOL.pairs; ++i) {
    order.push(i % 2 ? ["fresh", "retained"] : ["retained", "fresh"]);
  }
  assert.deepEqual(f.observations, order.flat());
  assert.deepEqual(f.lifecycle, ["open", "close"]);
  assert.equal(r.warmups.length, 4);
  assert.equal(r.pairs.length, 7);
  assert.equal(r.diagnosticOnly, true);
  assert.equal(r.complete, true);
  assert.ok(Math.abs(r.costs.local_ms.ratio - 0.5) < 1e-12);
  assert.equal(f.records[0].complete, false);
  assert.equal(f.records[0].pairs.length, 0);
  assert.equal(f.records.at(-1).complete, true);
});

for (const mutation of [r => { r.coroutine = true; }, r => { r.callback_calls = 1; },
  r => { r.center[0] = 2; }, r => { r.metrics.presentedFrames++; },
  r => { r.metrics.objectCount--; }, r => { r.local_ms = NaN; }]) {
  test(`invalid control observations are retained, never replaced: ${mutation}`, async () => {
    const f = fixture();
    const measure = f.arguments.measure;
    f.arguments.measure = async (participant, source) => {
      const r = await measure(participant, source);
      if (participant === f.fresh && f.observations.length > 4) mutation(r);
      return r;
    };
    await assert.rejects(diagnoseWorkerHistory(f.arguments));
    assert.equal(f.records.at(-1).complete, false);
    assert.ok(f.records.at(-1).pairs.length > 0);
    assert.deepEqual(f.lifecycle, ["open", "close"]);
  });
}

test("a failed run is not retried and always closes the fresh participant", async () => {
  const f = fixture();
  const failure = new Error("worker failed");
  f.arguments.measure = async () => { f.observations.push("failed"); throw failure; };
  await assert.rejects(diagnoseWorkerHistory(f.arguments), e => e === failure);
  assert.equal(f.observations.length, 1);
  assert.equal(f.records.at(-1).complete, false);
  assert.deepEqual(f.lifecycle, ["open", "close"]);
});

test("control rejects missing source identity before starting a worker", async () => {
  const f = fixture();
  f.arguments.identity.source = "";
  await assert.rejects(diagnoseWorkerHistory(f.arguments), /verified control source/);
  assert.deepEqual(f.lifecycle, []);
});

test("history diagnostics run after all accepted pairs and never alter their results", async () => {
  const runner = await readFile(new URL("./python-host-perf.mjs", import.meta.url), "utf8");
  const start = runner.indexOf("  // Same-package history control AFTER");
  assert.ok(start > runner.indexOf("localDiagnostics.push"));
  assert.ok(start > runner.indexOf("qualifyProductMetrics(comparison)"));
  const block = runner.slice(start, runner.indexOf('  failures.push({ kind: "execution"', start));
  assert.match(block, /identity: identities\[side\]/);
  assert.match(block, /servers\[side\]\.baseUrl/);
  assert.match(block, /retained: \{ page: pages\[side\], reports: reports\[side\] \}/);
  assert.match(block, /browser\.newContext\(/);
  assert.match(block, /worker-history-\$\{side\}\.json/);
  assert.doesNotMatch(block, /rows\.push|pairedCost\(|warmups\.push|failures\.length\s*=|failures\.splice/);
});


test("CPU sampling follows every history pair, reuses exact source, and cannot rescore it", async () => {
  const f = fixture(); const measured = [];
  f.arguments.sampleWorker = async ({ run, record }) => {
    const previous = f.records.at(-1);
    assert.equal(previous.complete, true);
    assert.equal(previous.pairs.length, PERF_PROTOCOL.pairs);
    const originalPairs = structuredClone(previous.pairs);
    const result = await run(); measured.push(result);
    await record({ diagnosticOnly: true, complete: true, observation: result });
    assert.deepEqual(f.records.at(-1).pairs, originalPairs);
  };
  const result = await diagnoseWorkerHistory(f.arguments);
  assert.equal(measured.length, 2);
  assert.equal(f.observations.length, 2 * (PERF_PROTOCOL.warmups + PERF_PROTOCOL.pairs) + 2);
  assert.equal(result.workerProfilesComplete, true);
  assert.deepEqual(Object.keys(result.workerProfiles), ["retained", "fresh"]);
  assert.deepEqual(f.lifecycle, ["open", "close"]);
});

test("sampling failure retains complete history evidence and closes the fresh worker", async () => {
  const f = fixture(); const failure = new Error("sampling unavailable");
  f.arguments.sampleWorker = async ({ record }) => {
    assert.equal(f.records.at(-1).complete, true);
    assert.equal(f.records.at(-1).pairs.length, 7);
    await record({ diagnosticOnly: true, complete: false, error: String(failure) });
    throw failure;
  };
  await assert.rejects(diagnoseWorkerHistory(f.arguments), error => error === failure);
  assert.deepEqual(f.lifecycle, ["open", "close"]);
  assert.equal(f.records.at(-1).pairs.length, 7);
  assert.equal(f.records.at(-1).workerProfilesComplete, false);
});


test("controlled history starts fresh peers and isolates only declared treatment history", async () => {
  const lifecycle = [], records = [], measured = [];
  const make = name => ({ name, reports: [], close: async () => lifecycle.push(`close-${name}`) });
  const source = performanceSource("jspi", "deterministic");
  const history = Array.from({ length: 3 }, () => ({ mode: "jspi", workload: "deterministic", source }));
  const result = await diagnoseControlledHistory({
    identity: { source: "b".repeat(40) }, label: "same_source",
    openFresh: async name => { lifecycle.push(`open-${name}`); return make(name); },
    history, record: async value => records.push(structuredClone(value)),
    measure: async (participant, measuredSource, mode, workload) => {
      assert.equal(measuredSource, source);
      assert.equal(mode, "jspi"); assert.equal(workload, "deterministic");
      measured.push(participant.name);
      return { mode, workload, coroutine: false, callback_calls: 0, center: [1.2048, 0],
        metrics: { presentedFrames: 50, objectCount: PERF_PROTOCOL.objects },
        creation_ms: 10, local_ms: participant.name === "treatment" ? 12 : 10, execution_ms: 20 };
    },
  });
  assert.deepEqual(lifecycle.slice(0, 2), ["open-control", "open-treatment"]);
  assert.equal(result.treatmentHistory.length, 3);
  assert.equal(result.pairs.length, PERF_PROTOCOL.pairs);
  assert.equal(result.complete, true);
  assert.ok(result.costs.local_ms.ratio > 1);
  assert.equal(measured.filter(name => name === "treatment").length,
    PERF_PROTOCOL.warmups + history.length + PERF_PROTOCOL.pairs);
  assert.equal(measured.filter(name => name === "control").length,
    PERF_PROTOCOL.warmups + PERF_PROTOCOL.pairs);
  assert.deepEqual(lifecycle.slice(-2), ["close-treatment", "close-control"]);
  assert.equal(records.at(-1).complete, true);
});

test("controlled-history diagnostics are after scored rows and cannot rescore them", async () => {
  const runner = await readFile(new URL("./python-host-perf.mjs", import.meta.url), "utf8");
  const controlled = runner.indexOf("  // Fresh-peer causal controls.");
  assert.ok(controlled > runner.indexOf("localDiagnostics.push"));
  assert.ok(controlled > runner.indexOf("qualifyProductMetrics(comparison)"));
  const block = runner.slice(controlled, runner.indexOf('  failures.push({ kind: "execution"', controlled));
  assert.match(block, /diagnoseControlledHistory/);
  assert.match(block, /repeated_same_source/);
  assert.match(block, /heterogeneous_sources/);
  assert.doesNotMatch(block, /rows\.push|pairedCost\(|warmups\.push|failures\.splice/);
});


test("scored fixed-work pairs warm the exact fresh workers without changing budgets", async () => {
  const runner = await readFile(new URL("./python-host-perf.mjs", import.meta.url), "utf8");
  const scored = runner.indexOf("      // Every scored pair uses two new workers.");
  const profiles = runner.indexOf("  // Profiles are a separate diagnostic experiment");
  assert.ok(scored > 0 && profiles > scored);
  const block = runner.slice(scored, profiles);
  assert.match(block, /openScoredParticipant/);
  assert.match(block, /scoredPairSchedule\(pair \+ 1\)/);
  assert.match(block, /event\.kind === "warmup"/);
  assert.match(block, /measure\(event\.side, source, mode, workload, participants\[event\.side\]\)/);
  assert.match(block, /workerLifetime: "fresh-per-pair-warmed"/);
  assert.match(block, /Promise\.all\(participants\.map\(participant => participant\.close\(\)\)\)/);
  assert.match(block, /protocol\.pairs/);
  assert.doesNotMatch(block, /maxPointRatio\s*=|maxUpperRatio\s*=|pairs\s*=\s*[0-9]/);
});
