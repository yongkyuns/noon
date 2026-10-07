// Strict qualification of every declared adjacent-base and cumulative-anchor cohort.
// This consumes retained evidence only: no browser runs, retries or new samples.
import assert from "node:assert/strict";
import { readProductPerformanceAnchor } from "../.github/ci/product-performance-anchor.mjs";
import { productMeasurement } from "./playground-product-fps.mjs";
import { CUMULATIVE_PRODUCT_PAIR_COUNT, PRODUCT_PAIR_COUNT,
  qualifyProductMetrics } from "./paired-product-metrics.mjs";

const latencyNames = ["shell ready", "cold Run → applied", "warm Run → applied", "edit → applied"];

export async function qualifyProductCohorts(readComparison) {
  const { workloads } = await readProductPerformanceAnchor();
  const cohorts = [], failures = [];
  // Evaluate both fixed scopes independently, even when an earlier scope fails.
  // The three-pair anchor is additional evidence, never a substitute for seven
  // adjacent-base pairs. This only reads completed comparisons; it runs no trials.
  const scopes = [["adjacent", PRODUCT_PAIR_COUNT], ["cumulative", CUMULATIVE_PRODUCT_PAIR_COUNT]];
  for (const [scope, expectedPairs] of scopes) {
    for (const exampleId of workloads) {
      const leaf = exampleId === "parity-square-and-circle" ? "."
        : exampleId === "showcase-camera-follows-path" ? "camera" : exampleId;
      const directory = scope === "adjacent" ? leaf
        : leaf === "." ? "cumulative" : `cumulative/${leaf}`;
      const cohortFailures = [];
      let metrics = null, latency = null;
      try {
        const comparison = await readComparison(directory);
        assert.equal(comparison.exampleId, exampleId, "product cohort example changed");
        const measurement = productMeasurement(exampleId);
        for (const pair of comparison.measurements ?? []) {
          for (const side of ["baseline", "candidate"]) {
            const report = pair[side];
            assert.equal(report?.exampleId, exampleId, "product report example changed");
            assert.deepEqual(report.measurement, measurement, "product measurement protocol changed");
            assert.equal(report.source?.path, measurement.sourcePath, "product source path changed");
            assert.match(report.source?.sha256 ?? "", /^[0-9a-f]{64}$/, "product source hash is missing");
            assert.deepEqual(report.source, comparison.measurements[0].baseline.source,
              "product cohort must retain identical authored source");
          }
        }
        metrics = qualifyProductMetrics(comparison, expectedPairs);
        const { fps, render } = metrics;
        if (fps.status !== "pass") cohortFailures.push({
          kind: fps.status === "regression" ? "product_fps" : "product_fps_inconclusive", ...fps,
        });
        if (render !== null && render.status !== "pass") {
          cohortFailures.push({ kind: "product_render_cost", ...render });
        }
        latency = comparison.latency;
        assert.deepEqual(Object.keys(latency ?? {}).sort(), [...latencyNames].sort(),
          "product latency evidence must retain all four metrics");
        for (const name of latencyNames) {
          const value = latency[name];
          assert.ok([value?.baselineMs, value?.candidateMs].every(x => Number.isFinite(x) && x >= 0),
            "product latency evidence must be finite and non-negative");
          if (value.candidateMs > value.baselineMs * 1.03 + 20) {
            cohortFailures.push({ kind: "product_latency", name, ...value });
          }
        }
      } catch (error) {
        // Retain malformed/missing cohorts as failures and still inspect the rest.
        // No failed input is dropped, replaced or re-read to manufacture a pass.
        cohortFailures.push({ kind: "product_evidence", message: String(error) });
      }
      const qualified = cohortFailures.map(failure => ({ scope, cohort: directory, exampleId, ...failure }));
      failures.push(...qualified);
      cohorts.push({ scope, expectedPairs, exampleId, directory, metrics, latency,
        status: qualified.length === 0 ? "pass" : "blocked", failures: qualified });
    }
  }
  return { schema: 2, cohorts, failures };
}
