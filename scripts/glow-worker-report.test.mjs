import test from "node:test";
import assert from "node:assert/strict";
import { encodeGlowWorkerReport } from "./glow-worker-report.mjs";

test("worker report preserves nested u64 counters as exact decimal text", () => {
  const report = {
    status: "passed", frames: 50, maxError: 2,
    modes: [{ mode: "transferable", metrics: {
      presentedFrames: 29n,
      gpuIdentity: { fingerprint: 18446744073709551615n },
      timings: [-1n, 0n, 9007199254740993n],
      supported: true,
    } }],
  };
  const json = encodeGlowWorkerReport(report);
  const decoded = JSON.parse(json);
  assert.equal(json.endsWith("\n"), true);
  assert.deepEqual(decoded, {
    status: "passed", frames: 50, maxError: 2,
    modes: [{ mode: "transferable", metrics: {
      presentedFrames: "29",
      gpuIdentity: { fingerprint: "18446744073709551615" },
      timings: ["-1", "0", "9007199254740993"],
      supported: true,
    } }],
  });
});

test("failure reports serialize without converting normal numbers or masking invalid JSON", () => {
  assert.deepEqual(JSON.parse(encodeGlowWorkerReport({
    status: "failed", error: "worker rejected invalid glow", maxError: 2, contexts: [null, 0, false],
  })), {
    status: "failed", error: "worker rejected invalid glow", maxError: 2, contexts: [null, 0, false],
  });
  const cycle = {}; cycle.self = cycle;
  assert.throws(() => encodeGlowWorkerReport(cycle), /circular/i);
});
