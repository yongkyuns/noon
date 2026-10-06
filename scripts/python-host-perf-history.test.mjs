import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { diagnoseWorkerHistory } from "./python-host-perf-history.mjs";
import { PERF_PROTOCOL, performanceSource } from "./python-host-perf-protocol.mjs";

function fixture() {
  const observations = [], records = [], lifecycle = [];
  const retained = { name: "retained" };
  const fresh = { name: "fresh", close: async () => { lifecycle.push("close"); } };
  return { observations, records, lifecycle, retained, fresh,
    arguments: { identity: { source: "a".repeat(40) }, retained,
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
