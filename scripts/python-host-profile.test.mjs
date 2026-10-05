import assert from "node:assert/strict";
import test from "node:test";
import { profileSource, validateProfile } from "./python-host-profile.mjs";
import { performanceSource, PERF_PROTOCOL } from "./python-host-perf-protocol.mjs";

test("profiling preserves the entire measured source and its workload", () => {
  for (const workload of PERF_PROTOCOL.workloads) {
    const source = profileSource(workload);
    const original = source.slice(0, source.indexOf("        _noon_profile.disable()"))
      .replace("import cProfile\n", "")
      .replace("        _noon_profile = cProfile.Profile()\n        _noon_profile.enable()\n", "");
    assert.equal(original, performanceSource("async", workload));
    assert.match(source, /"timing_evidence": False/);
  }
  assert.throws(() => profileSource("unknown"));
});

test("profile evidence rejects wrong workloads, empty and invalid records", () => {
  const report = () => ({ workload: "deterministic", timing_evidence: false,
    rows: [{ file: "noon.py", name: "shift", line: 1, calls: 2, recursive_calls: 0,
      self_seconds: 0.001, cumulative_seconds: 0.002 }] });
  assert.equal(validateProfile(report(), "deterministic").rows.length, 1);
  for (const mutate of [r => r.workload = "callbacks", r => r.timing_evidence = true,
    r => r.rows = [], r => r.rows[0].calls = -1, r => r.rows[0].recursive_calls = 3,
    r => r.rows[0].self_seconds = NaN, r => r.rows[0].cumulative_seconds = "0.1"]) {
    const value = report(); mutate(value);
    assert.throws(() => validateProfile(value, "deterministic"));
  }
});
