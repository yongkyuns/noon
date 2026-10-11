import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { profileSource, validateProfile, localEditProfileSource, validateLocalEditProfile } from "./python-host-profile.mjs";
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


for (const mode of PERF_PROTOCOL.modes) {
  test(`local-edit profile preserves the exact ${mode} program outside diagnostics`, () => {
    const source = localEditProfileSource(mode);
    const original = source.slice(0, source.indexOf("        _local_rows = []"))
      .replace("import cProfile\nimport gc\n", "")
      .replace(/        _local_profile = cProfile.Profile\(\)[\s\S]*?        _local_profile.enable\(\)\n/, "")
      .replace(/        _local_profile.disable\(\)[\s\S]*?        _local_collections_after = .*\n/, "");
    assert.equal(original, performanceSource(mode, "deterministic"));
    const profiled = source.slice(source.indexOf("        _local_profile.enable()"),
      source.indexOf("        _local_profile.disable()"));
    assert.match(profiled, new RegExp(`for _ in range\\(${PERF_PROTOCOL.localEdits}\\)`));
    assert.match(profiled, /dots\[0\]\.shift\(0\.0001 \* RIGHT\)/);
    assert.match(profiled, /dots\[0\]\.get_center\(\)/);
    assert.doesNotMatch(profiled, /await|\.play\(|\.wait\(|\._play\(|\._wait\(/);
    assert.doesNotMatch(source, /gc\.(collect|disable|enable|set_threshold)\(/);
    assert.match(source, /"timing_evidence": False/);
    assert.match(source, new RegExp(`"mode": "${mode}"`));
  });
}

test("local-edit evidence validates mode, scope, rows and observational GC counters", () => {
  const report = () => ({ workload: "deterministic", mode: "jspi", scope: "local_edit_only",
    timing_evidence: false, gc_counts_before: [1, 2, 3], gc_counts_after: [0, 3, 3],
    gc_collections_before: [20, 2, 0], gc_collections_after: [21, 2, 0],
    rows: [{ file: "noon.py", name: "shift", line: 1, calls: 2048, recursive_calls: 0,
      self_seconds: 0.001, cumulative_seconds: 0.002 }] });
  assert.equal(validateLocalEditProfile(report(), "jspi").mode, "jspi");
  for (const change of [r => r.mode = "async", r => r.scope = "whole_scene",
    r => r.timing_evidence = true, r => r.rows = [], r => r.gc_counts_after = [],
    r => r.gc_counts_before[0] = 1.5, r => r.gc_counts_after[1] = "3",
    r => r.gc_collections_after[0] = 19, r => delete r.gc_collections_before]) {
    const value = report(); change(value);
    assert.throws(() => validateLocalEditProfile(value, "jspi"));
  }
  assert.throws(() => validateLocalEditProfile(report(), "unknown"));
  assert.throws(() => localEditProfileSource("unknown"));
});

test("local-edit profiles run after qualifications and cannot enter accepted timing rows", async () => {
  const runner = await readFile(new URL("./python-host-perf.mjs", import.meta.url), "utf8");
  const start = runner.indexOf("    const source = localEditProfileSource(mode);");
  assert.ok(start > runner.indexOf("    const { fps, render } = qualifyProductMetrics(comparison);"));
  assert.ok(start > runner.indexOf("      const source = profileSource(workload);"));
  const block = runner.slice(start, runner.indexOf("} catch (error)", start));
  assert.doesNotMatch(block, /rows\.push|pairedCost\(|warmups\.push/);
  assert.match(block, /validateLocalEditProfile\(localProfiles\[side\]\.at\(-1\), mode\)/);
  assert.match(block, /"local-edit-profiles\.json"/);
  assert.match(block, /local-edit-profile-\$\{mode\}\.py/);
});
