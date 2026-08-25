import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const [baselinePath, candidatePath] = process.argv.slice(2);
assert.ok(baselinePath && candidatePath, "usage: node scripts/perf-compare.mjs BASELINE.json CANDIDATE.json");
const baseline = JSON.parse(await readFile(baselinePath, "utf8"));
const candidate = JSON.parse(await readFile(candidatePath, "utf8"));
assert.equal(baseline.benchmark, candidate.benchmark, "artifacts must be from the same benchmark");

const rows = baseline.benchmark.includes("authoring")
  ? compareAuthoring(baseline, candidate)
  : compareFrames(baseline, candidate);

console.log("| Case | Metric | Baseline | Candidate | Delta |");
console.log("|---|---|---:|---:|---:|");
for (const row of rows) {
  console.log(`| ${row.caseName} | ${row.metric} | ${fmt(row.before)} | ${fmt(row.after)} | ${fmtDelta(row.deltaPct)} |`);
}

const threshold = Number(process.env.NOON_PERF_REGRESSION_PCT ?? "NaN");
if (Number.isFinite(threshold)) {
  const regressions = rows.filter((row) => row.regressionPct > threshold);
  if (regressions.length > 0) {
    console.error(`${regressions.length} metric(s) regressed by more than ${threshold}%`);
    process.exitCode = 2;
  }
}

function compareFrames(before, after) {
  const candidateCases = new Map(after.cases.map((item) => [frameKey(item), item]));
  const rows = [];
  for (const oldCase of before.cases) {
    const key = frameKey(oldCase);
    const next = candidateCases.get(key);
    if (!next) continue;
    const metrics = [
      ["frame p95 ms", oldCase.cadence?.frameIntervalMs?.p95, next.cadence?.frameIntervalMs?.p95, false],
      ["frame p99 ms", oldCase.cadence?.frameIntervalMs?.p99, next.cadence?.frameIntervalMs?.p99, false],
      ["effective FPS", oldCase.cadence?.effective?.effectiveFps, next.cadence?.effective?.effectiveFps, true],
      ["CPU p95 ms", oldCase.cpu?.frameMs?.p95, next.cpu?.frameMs?.p95, false],
      ["GPU p95 ms", oldCase.gpu?.renderPassMs?.p95, next.gpu?.renderPassMs?.p95, false],
    ];
    for (const [metric, oldValue, newValue, higherBetter] of metrics) {
      pushMetric(rows, key, metric, oldValue, newValue, higherBetter);
    }
  }
  return rows;
}

function compareAuthoring(before, after) {
  const candidateCases = new Map(after.cases.map((item) => [String(item.workload.objects), item]));
  const rows = [];
  for (const oldCase of before.cases) {
    const key = String(oldCase.workload.objects);
    const next = candidateCases.get(key);
    if (!next) continue;
    const metrics = [
      ["unchanged visible p95 ms", oldCase.warmUnchanged?.timeToVisibleMs?.p95, next.warmUnchanged?.timeToVisibleMs?.p95],
      ["local edit visible p95 ms", oldCase.oneObjectEdit?.timeToVisibleMs?.p95, next.oneObjectEdit?.timeToVisibleMs?.p95],
      ["scrub p95 ms", oldCase.scrub?.timeToVisibleMs?.p95, next.scrub?.timeToVisibleMs?.p95],
      ["serialize p95 ms", oldCase.oneObjectEdit?.serializeMs?.p95, next.oneObjectEdit?.serializeMs?.p95],
      ["reconcile p95 ms", oldCase.oneObjectEdit?.reconcileMs?.p95, next.oneObjectEdit?.reconcileMs?.p95],
    ];
    for (const [metric, oldValue, newValue] of metrics) {
      pushMetric(rows, `${Number(key).toLocaleString()} objects`, metric, oldValue, newValue, false);
    }
  }
  return rows;
}

function pushMetric(rows, caseName, metric, before, after, higherBetter) {
  if (!Number.isFinite(before) || !Number.isFinite(after) || before === 0) return;
  const deltaPct = ((after - before) / before) * 100;
  rows.push({
    caseName,
    metric,
    before,
    after,
    deltaPct,
    regressionPct: higherBetter ? -deltaPct : deltaPct,
  });
}

function frameKey(item) {
  return `${item.workload.layout}/${item.workload.objects}@${item.environment.backingResolution.join("x")}`;
}

function fmt(value) {
  return Number.isFinite(value) ? Number(value).toFixed(3) : "—";
}

function fmtDelta(value) {
  return Number.isFinite(value) ? `${value >= 0 ? "+" : ""}${value.toFixed(1)}%` : "—";
}
