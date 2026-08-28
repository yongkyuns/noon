import assert from "node:assert/strict";
import test from "node:test";

import { readCompletePlayerFrameMetrics } from "./player-frame-metrics.js";

function player(overrides = {}) {
  const values = {
    cpuFrameMs: 4.5,
    runtimeMs: 1.25,
    prepareMs: 0.75,
    uploadMs: 0.5,
    encodeSubmitMs: 0.9,
    ...overrides,
  };
  return {
    lastCpuFrameMs: () => values.cpuFrameMs,
    lastRuntimeEvaluationMs: () => values.runtimeMs,
    lastFramePrepareMs: () => values.prepareMs,
    lastUploadMs: () => values.uploadMs,
    lastEncodeSubmitMs: () => values.encodeSubmitMs,
  };
}

test("returns one complete frame metric snapshot", () => {
  assert.deepEqual(readCompletePlayerFrameMetrics(player()), {
    cpuFrameMs: 4.5,
    runtimeMs: 1.25,
    prepareMs: 0.75,
    uploadMs: 0.5,
    encodeSubmitMs: 0.9,
  });
});

test("rejects the whole frame when any metric is unavailable", () => {
  assert.equal(
    readCompletePlayerFrameMetrics(player({ runtimeMs: Number.NaN })),
    null,
  );
  assert.equal(
    readCompletePlayerFrameMetrics(player({ uploadMs: Number.POSITIVE_INFINITY })),
    null,
  );
});
