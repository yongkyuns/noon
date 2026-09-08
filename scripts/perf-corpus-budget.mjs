/** Evaluate only actual measurements; unavailable timing cannot pass a gate. */
export function evaluateBudget(report, budget) {
  if (budget === null) return { passed: true, complete: true, gated: false, checks: [] };
  const actual = {
    frameIntervalP95Ms: report.cadence.frameIntervalMs?.p95,
    frameIntervalP99Ms: report.cadence.frameIntervalMs?.p99,
    longFrameRateMax: report.cadence.effective?.longFrameRate,
    advanceRoundTripP95Ms: report.pipeline?.advanceRoundTripMs?.p95,
  };
  const checks = Object.entries(budget).map(([metric, limit]) => {
    const measured = Number.isFinite(actual[metric]);
    return {
      metric, limit, actual: measured ? actual[metric] : null,
      status: !measured ? "unavailable" : actual[metric] <= limit ? "passed" : "failed",
    };
  });
  return {
    passed: checks.every((check) => check.status === "passed"),
    complete: checks.every((check) => check.status !== "unavailable"),
    gated: true,
    checks,
  };
}
