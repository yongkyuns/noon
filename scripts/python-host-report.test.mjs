import assert from "node:assert/strict";
import test from "node:test";
import { assertCompletedSample, assertRenderedOutput, assertSingleReport, stringifyEvidence } from "./python-host-report.mjs";

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

test("renderer assertions distinguish CLI selectors from runtime names", () => {
  assert.doesNotThrow(() => assertRenderedOutput({ backend: "WebGPU", presentedFrames: 2n, drawCalls: 1 }, "webgpu"));
  assert.doesNotThrow(() => assertRenderedOutput({ backend: "WebGL2", presentedFrames: 2, drawCalls: 1 }, "webgl"));
  assert.throws(() => assertRenderedOutput({ backend: "WebGPU", presentedFrames: 2, drawCalls: 1 }, "webgl"), /requested backend/);
  assert.throws(() => assertRenderedOutput({ backend: "WebGPU", presentedFrames: 0, drawCalls: 1 }, "webgpu"), /no actual rendered output/);
  assert.throws(() => assertRenderedOutput({ backend: "WebGPU", presentedFrames: 2, drawCalls: 0 }, "webgpu"), /no actual rendered output/);
  assert.throws(() => assertRenderedOutput({}, "unknown"), /unknown renderer selection/);
});

test("a quiet final hold needs a logical receipt, not a redundant redraw", () => {
  const metrics = { backend: "WebGPU", time: 0.5, presentedFrames: 3, drawCalls: 1 };
  assertRenderedOutput(metrics, "webgpu");
  assert.doesNotThrow(() => assertCompletedSample([{ time: 1, sourceCompleted: true }], 1));
  assert.throws(() => assertCompletedSample([], 1), /missing source-completion/);
  assert.throws(() => assertCompletedSample([{ time: 1, sourceCompleted: false }], 1), /missing source-completion/);
  for (const time of [0.75, 1.25, NaN, Infinity]) {
    assert.throws(() => assertCompletedSample([{ time, sourceCompleted: true }], 1), /final authored time/);
  }
});
