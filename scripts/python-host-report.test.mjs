import assert from "node:assert/strict";
import test from "node:test";
import { assertSingleReport, stringifyEvidence } from "./python-host-report.mjs";

test("u64 metrics survive evidence serialization without precision loss or mutation", () => {
  const result = { metrics: { epoch: 18446744073709551615n }, samples: [0n, 1n] };
  assert.deepEqual(JSON.parse(stringifyEvidence(result)), {
    metrics: { epoch: "18446744073709551615" }, samples: ["0", "1"],
  });
  assert.equal(typeof result.metrics.epoch, "bigint");
});

test("a passing report count does not fail while evaluating its diagnostic", () => {
  assert.doesNotThrow(() => assertSingleReport([{}], "sequential", { frames: 1n }));
});

test("missing and duplicate reports retain the assertion and runtime diagnostic", () => {
  for (const output of [[], [{}, {}]]) {
    assert.throws(() => assertSingleReport(output, "sequential", { frames: 2n }), error => {
      assert.equal(error.code, "ERR_ASSERTION");
      assert.match(error.message, /expected exactly one Python report/);
      assert.match(error.message, /"frames": "2"/);
      return true;
    });
  }
});
