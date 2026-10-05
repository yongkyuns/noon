// Diagnostic/export boundary only. Keep u64 runtime counters lossless without
// changing the typed observations used by the conformance assertions.
import assert from "node:assert/strict";

export function stringifyEvidence(value) {
  return JSON.stringify(value, (_key, item) =>
    typeof item === "bigint" ? item.toString() : item, 2);
}

export function assertSingleReport(output, name, result) {
  assert.equal(output.length, 1,
    `${name}: expected exactly one Python report; ${stringifyEvidence(result)}`);
}
