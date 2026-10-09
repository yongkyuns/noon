import test from "node:test";
import assert from "node:assert/strict";
import { assess, PINNED_CASES, gpuProbeSource, STUDY_ID } from "./perf-standard-macos-webgl-feas.mjs";

const ok = () => ({
  browser: "151.0.7922.34", backend: "WebGL2",
  unmaskedVendor: "Apple Inc.", unmaskedRenderer: "Apple M1 GPU",
  clearPixel: [51, 102, 153, 255], trianglePixel: [153, 51, 102, 255],
  shaderLinked: true, glError: 0, contextLost: false,
});

test("the only registered modes are headed then headless", () => {
  assert.deepEqual(PINNED_CASES, [
    { id: "headed", headless: false }, { id: "headless", headless: true },
  ]);
  assert.match(STUDY_ID, /standard-macos-webgl/);
  assert.equal(typeof gpuProbeSource(), "function");
});

test("nonsoftware renderer with real pixel proof is only eligible for a separate diagnostic", () => {
  for (const mode of PINNED_CASES) {
    const result = assess(mode.id, ok());
    assert.equal(result.status, "eligible-for-separate-aa-only");
    assert.equal(result.classification, "hardware-like-unverified");
    assert.equal(result.physicalGpuProven, false);
    assert.equal(result.qualification, false);
    assert.equal(result.performanceAcceptance, false);
    assert.equal(result.mergeApproval, false);
  }
  assert.throws(() => assess("fallback-on-demand", ok()));
});

test("software fallback, missing WebGL2, bad pixels and mismatched browser fail closed", () => {
  const mutate = [
    p => p.unmaskedRenderer = "ANGLE (Google SwiftShader Device)",
    p => p.unmaskedRenderer = "Mesa llvmpipe (LLVM)",
    p => p.unmaskedRenderer = "",
    p => p.unmaskedVendor = "",
    p => p.backend = "missing",
    p => p.clearPixel = [0, 0, 0, 0],
    p => p.trianglePixel = [0, 0, 0, 0],
    p => p.shaderLinked = false,
    p => p.contextLost = true,
    p => p.glError = 1280,
    p => p.browser = "152.0.0.0",
  ];
  for (const fn of mutate) {
    const v = ok(); fn(v);
    const result = assess("headed", v);
    assert.equal(result.status, "not-confirmed");
    assert.ok(result.blockers.length > 0);
  }
  assert.equal(assess("headless", null).status, "not-confirmed");
});
