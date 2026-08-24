import assert from "node:assert/strict";
import test from "node:test";

import { ANALYTIC_LAYOUTS, buildAnalyticScene } from "./perf-workloads.js";

test("analytic workloads are deterministic and preserve requested object count", () => {
  for (const layout of ANALYTIC_LAYOUTS) {
    const first = buildAnalyticScene({ count: 100, layout, aspect: 16 / 9 });
    const second = buildAnalyticScene({ count: 100, layout, aspect: 16 / 9 });
    assert.deepEqual(first, second);
    assert.equal(first.document.objects.length, 100);
    assert.deepEqual(first.document.tracks, []);
    assert.ok(first.cameraHeight > 0);
  }
});

test("fit workload grows camera height while fixed workload preserves apparent radius", () => {
  const fitSmall = buildAnalyticScene({ count: 100, layout: "fit" });
  const fitLarge = buildAnalyticScene({ count: 10_000, layout: "fit" });
  assert.ok(fitLarge.cameraHeight > fitSmall.cameraHeight);

  const fixedSmall = buildAnalyticScene({ count: 100, layout: "fixed" });
  const fixedLarge = buildAnalyticScene({ count: 10_000, layout: "fixed" });
  assert.equal(fixedSmall.cameraHeight, fixedLarge.cameraHeight);
  assert.equal(
    fixedSmall.document.objects[0].geometry.circle.radius,
    fixedLarge.document.objects[0].geometry.circle.radius,
  );
});

test("overdraw workload keeps transparent objects concentrated near the origin", () => {
  const result = buildAnalyticScene({ count: 1_000, layout: "overdraw" });
  for (const object of result.document.objects) {
    const { x, y } = object.transform.translation;
    assert.ok(Math.hypot(x, y) <= 0.401);
    assert.equal(object.style.fill.alpha, 0.16);
  }
});

test("analytic workload parameters reject invalid values", () => {
  assert.throws(() => buildAnalyticScene({ count: 0 }), /positive integer/);
  assert.throws(() => buildAnalyticScene({ count: 1, layout: "unknown" }), /unknown analytic layout/);
  assert.throws(() => buildAnalyticScene({ count: 1, aspect: 0 }), /positive finite/);
});
