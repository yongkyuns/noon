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
