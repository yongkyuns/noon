// Diagnostic-only source. Run after ALL prescribed timing pairs, never instead
// of them: profiling changes interpreter costs and cannot establish performance.
import assert from "node:assert/strict";
import { performanceSource } from "./python-host-perf-protocol.mjs";

export function profileSource(workload) {
  const source = performanceSource("async", workload);
  const start = "        started = perf_counter()";
  assert.equal(source.split(start).length, 4, "unexpected benchmark timing boundaries");
  return source.replace("import json\n", "import json\nimport cProfile\n")
    .replace(start, "        _noon_profile = cProfile.Profile()\n        _noon_profile.enable()\n" + start)
    + `        _noon_profile.disable()
        _noon_rows = []
        for _entry in _noon_profile.getstats():
            _code = _entry.code
            _noon_rows.append({"file": getattr(_code, "co_filename", "<builtin>"),
                "line": getattr(_code, "co_firstlineno", 0),
                "name": getattr(_code, "co_name", str(_code)),
                "calls": _entry.callcount, "recursive_calls": _entry.reccallcount,
                "self_seconds": _entry.inlinetime, "cumulative_seconds": _entry.totaltime})
        print("NOON_PERF_PROFILE " + json.dumps({"workload": "${workload}",
            "timing_evidence": False, "rows": _noon_rows}))
`;
}

export function validateProfile(value, workload) {
  assert.equal(value?.workload, workload, "profile workload mismatch");
  assert.equal(value.timing_evidence, false, "profiles are never timing acceptance evidence");
  assert.ok(Array.isArray(value.rows) && value.rows.length > 0, "empty profile");
  for (const row of value.rows) {
    assert.equal(typeof row.file, "string");
    assert.equal(typeof row.name, "string");
    for (const key of ["line", "calls", "recursive_calls"]) {
      assert.ok(Number.isSafeInteger(row[key]) && row[key] >= 0, `invalid profile ${key}`);
    }
    for (const key of ["self_seconds", "cumulative_seconds"]) {
      assert.ok(Number.isFinite(row[key]) && row[key] >= 0, `invalid profile ${key}`);
    }
    assert.ok(row.recursive_calls <= row.calls, "invalid recursive count");
  }
  return value;
}

// Profile only the uninterrupted local-edit block. In particular, no Python
// tracing is left active while a genuine JSPI stack is suspended. These runs
// are diagnostics after every timing pair; their durations are never accepted.
export function localEditProfileSource(mode) {
  const source = performanceSource(mode, "deterministic");
  const start = "        started = perf_counter()\n        for _ in range(";
  const end = "        local_ms = (perf_counter() - started) * 1000\n";
  assert.equal(source.split(start).length, 2, "local-edit start boundary changed");
  assert.equal(source.split(end).length, 2, "local-edit end boundary changed");
  return source.replace("import json\n", "import json\nimport cProfile\nimport gc\n")
    .replace(start, `        _local_profile = cProfile.Profile()
        _local_gc_before = gc.get_count()
        _local_collections_before = [row["collections"] for row in gc.get_stats()]
        _local_profile.enable()
` + start)
    .replace(end, end + `        _local_profile.disable()
        _local_gc_after = gc.get_count()
        _local_collections_after = [row["collections"] for row in gc.get_stats()]
`)
    + `        _local_rows = []
        for _entry in _local_profile.getstats():
            _code = _entry.code
            _local_rows.append({"file": getattr(_code, "co_filename", "<builtin>"),
                "line": getattr(_code, "co_firstlineno", 0),
                "name": getattr(_code, "co_name", str(_code)),
                "calls": _entry.callcount, "recursive_calls": _entry.reccallcount,
                "self_seconds": _entry.inlinetime, "cumulative_seconds": _entry.totaltime})
        print("NOON_PERF_LOCAL_PROFILE " + json.dumps({"workload": "deterministic",
            "mode": "${mode}", "scope": "local_edit_only", "timing_evidence": False,
            "gc_counts_before": _local_gc_before, "gc_counts_after": _local_gc_after,
            "gc_collections_before": _local_collections_before,
            "gc_collections_after": _local_collections_after, "rows": _local_rows}))
`;
}

export function validateLocalEditProfile(value, mode) {
  // Validate the requested mode as well as the received one.
  performanceSource(mode, "deterministic");
  validateProfile(value, "deterministic");
  assert.equal(value.mode, mode, "local-edit profile mode mismatch");
  assert.equal(value.scope, "local_edit_only", "local-edit profile scope mismatch");
  for (const key of ["gc_counts_before", "gc_counts_after",
    "gc_collections_before", "gc_collections_after"]) {
    const collectionCounter = key.startsWith("gc_collections_");
    assert.ok(Array.isArray(value[key]) && value[key].length === 3 &&
      value[key].every(count => Number.isSafeInteger(count) && (!collectionCounter || count >= 0)),
    `invalid local-edit profile ${key}`);
  }
  assert.ok(value.gc_collections_after.every((count, index) =>
    count >= value.gc_collections_before[index]), "GC collection counters moved backwards");
  return value;
}
