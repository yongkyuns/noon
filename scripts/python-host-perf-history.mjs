// Diagnostic owned by #1874: distinguish worker-history effects from a code
// change. Both participants load one already-verified package. These timings
// never replace the normal baseline/candidate measurements or their verdicts.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { PERF_PROTOCOL, pairedCost, performanceSource } from "./python-host-perf-protocol.mjs";
import { openAuthoringInspector, collectWorkerProfile } from "./python-host-perf-worker-profile.mjs";

export async function diagnoseWorkerHistory({ identity, retained, openFresh, measure, record, sampleWorker = collectWorkerProfile }) {
  assert.match(identity?.source ?? "", /^[0-9a-f]{40}$/, "missing verified control source");
  const source = performanceSource("jspi", "deterministic");
  const evidence = { schema: 1, diagnosticOnly: true,
    comparison: "fresh-over-retained / same-package / genuine-jspi",
    identity, sourceSha: createHash("sha256").update(source).digest("hex"),
    warmups: [], pairs: [], complete: false };
  await record(evidence);
  const fresh = await openFresh();
  assert.notEqual(fresh, retained, "history control requires an independent fresh worker");
  try {
    for (let warm = 0; warm < PERF_PROTOCOL.warmups; ++warm) {
      for (const side of warm % 2 ? [1, 0] : [0, 1]) {
        const observation = await measure(side ? fresh : retained, source);
        evidence.warmups.push({ side: side ? "fresh" : "retained", observation });
        await record(evidence);
      }
    }
    for (let pair = 0; pair < PERF_PROTOCOL.pairs; ++pair) {
      const observations = [];
      for (const side of pair % 2 ? [1, 0] : [0, 1]) {
        observations[side] = await measure(side ? fresh : retained, source);
      }
      // Persist even an invalid pair before asserting; do not replace it.
      evidence.pairs.push(observations);
      await record(evidence);
      const [before, after] = observations;
      for (const value of observations) {
        assert.equal(value.mode, "jspi");
        assert.equal(value.workload, "deterministic");
        assert.equal(value.coroutine, false, "control silently changed source mode");
        assert.equal(value.callback_calls, 0, "control introduced Python callbacks");
        assert.equal(value.metrics.objectCount, PERF_PROTOCOL.objects, "control changed object work");
      }
      assert.equal(after.metrics.presentedFrames, before.metrics.presentedFrames,
        "control changed frame work");
      assert.equal(after.center.length, before.center.length);
      assert.ok(before.center.every((v, i) => Number.isFinite(v) &&
        Number.isFinite(after.center[i]) && Math.abs(v - after.center[i]) < 2e-5),
      "control changed semantic result");
    }
    evidence.costs = Object.fromEntries(["creation_ms", "local_ms", "execution_ms"].map(key => [key,
      pairedCost(evidence.pairs.map(p => p[0][key]), evidence.pairs.map(p => p[1][key]))]));
    evidence.complete = true;
    await record(evidence);
    // Sample only after every uninstrumented history pair (and the scored
    // matrix in the caller). These raw profiles never enter costs/pairs above.
    // Keep fresh-worker ownership here so a failed inspector still tears down.
    evidence.workerProfiles = {};
    evidence.workerProfilesComplete = false;
    for (const [name, participant] of [["retained", retained], ["fresh", fresh]]) {
      await sampleWorker({
        open: () => openAuthoringInspector(participant.page.context().browser(), participant.page),
        run: () => measure(participant, source),
        record: async profile => {
          evidence.workerProfiles[name] = profile;
          await record(evidence);
        },
      });
    }
    evidence.workerProfilesComplete = true;
    await record(evidence);
    return evidence;
  } finally {
    await fresh.close();
  }
}


// Controlled diagnostic: both participants start fresh from the same verified
// package. Only the treatment receives the declared prehistory. The subsequent
// alternating pairs therefore isolate that history from the old worker's
// unrelated scored/profile history. This never contributes to acceptance.
export async function diagnoseControlledHistory({
  identity, openFresh, measure, history, label, record,
}) {
  assert.match(identity?.source ?? "", /^[0-9a-f]{40}$/, "missing verified control source");
  assert.ok(Array.isArray(history) && history.length > 0, "controlled history must be non-empty");
  assert.match(label ?? "", /^[a-z0-9_-]+$/, "controlled history requires a stable label");
  const source = performanceSource("jspi", "deterministic");
  const evidence = { schema: 1, diagnosticOnly: true,
    comparison: "treatment-over-control / fresh-peers / genuine-jspi",
    label, identity, sourceSha: createHash("sha256").update(source).digest("hex"),
    history: history.map(({ mode, workload, source }) => ({
      mode, workload, sourceSha: createHash("sha256").update(source).digest("hex"),
    })),
    warmups: [], treatmentHistory: [], pairs: [], complete: false };
  await record(evidence);
  const control = await openFresh("control");
  let treatment;
  try {
    treatment = await openFresh("treatment");
    assert.notEqual(control, treatment, "controlled history requires independent workers");
    for (let warm = 0; warm < PERF_PROTOCOL.warmups; ++warm) {
      for (const [name, participant] of warm % 2
        ? [["treatment", treatment], ["control", control]]
        : [["control", control], ["treatment", treatment]]) {
        const observation = await measure(participant, source, "jspi", "deterministic");
        evidence.warmups.push({ side: name, observation });
        await record(evidence);
      }
    }
    for (const item of history) {
      const observation = await measure(treatment, item.source, item.mode, item.workload);
      evidence.treatmentHistory.push({ mode: item.mode, workload: item.workload, observation });
      await record(evidence);
    }
    for (let pair = 0; pair < PERF_PROTOCOL.pairs; ++pair) {
      const observations = [];
      for (const side of pair % 2 ? [1, 0] : [0, 1]) {
        const participant = side ? treatment : control;
        observations[side] = await measure(participant, source, "jspi", "deterministic");
      }
      evidence.pairs.push(observations);
      await record(evidence);
      const [before, after] = observations;
      for (const value of observations) {
        assert.equal(value.mode, "jspi");
        assert.equal(value.workload, "deterministic");
        assert.equal(value.coroutine, false);
        assert.equal(value.callback_calls, 0);
        assert.equal(value.metrics.objectCount, PERF_PROTOCOL.objects);
      }
      assert.equal(after.metrics.presentedFrames, before.metrics.presentedFrames);
      assert.ok(before.center.every((v, i) => Number.isFinite(v) &&
        Number.isFinite(after.center[i]) && Math.abs(v - after.center[i]) < 2e-5));
    }
    evidence.costs = Object.fromEntries(["creation_ms", "local_ms", "execution_ms"].map(key => [key,
      pairedCost(evidence.pairs.map(p => p[0][key]), evidence.pairs.map(p => p[1][key]))]));
    evidence.complete = true;
    await record(evidence);
    return evidence;
  } finally {
    await treatment?.close();
    await control.close();
  }
}
