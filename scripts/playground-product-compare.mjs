import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { summarizeSamples } from "../web/frame-metrics.js";
import { productMeasurement, sampleRendererFps, samplePresentationGaps, sampleRendererCosts } from "./playground-product-fps.mjs";
import { summarizePackageSizes } from "../.github/ci/wasm-build.mjs";
import { isDeepStrictEqual } from "node:util";
const cumulativeAnchor = JSON.parse(await readFile(new URL("./playground-product-anchor.json", import.meta.url), "utf8"));

const [baselineDirArg, candidateDirArg, mode, count] = process.argv.slice(2);
assert.ok(baselineDirArg && candidateDirArg &&
  (mode === undefined || (mode === "--pairs" && count === "3")) && process.argv.length <= 6,
  "usage: node scripts/playground-product-compare.mjs BASELINE_DIR CANDIDATE_DIR [--pairs 3]");
const pairCount = mode === undefined ? 1 : 3;
const baselineDir = path.resolve(baselineDirArg);
const candidateDir = path.resolve(candidateDirArg);
const metricKeys = ["shellReadyMs", "coldRunMs", "warmRunMs", "editRunMs"];
const pairs = [];
for (let index = 1; index <= pairCount; index += 1) {
  const directories = [baselineDir, candidateDir].map(directory =>
    pairCount === 1 ? directory : path.join(directory, `trial-${index}`));
  const reports = await Promise.all(directories.map(async directory =>
    JSON.parse(await readFile(path.join(directory, "report.json"), "utf8"))));
  for (const [side, report] of reports.entries()) {
    const name = side === 0 ? "baseline" : "candidate";
    assert.equal(report?.schemaVersion, 2, `${name} product report has an unsupported schema`);
    assert.ok(typeof report.exampleId === "string" && report.exampleId.length > 0,
      `${name} product report must name an example`);
    assert.ok(typeof report.runtime?.backend === "string" && report.runtime.backend.length > 0,
      `${name} product report must name a renderer backend`);
    for (const key of metricKeys) {
      assert.ok(Number.isFinite(report[key]) && report[key] >= 0,
        `${name} ${key}: latency must be finite and non-negative`);
    }
    assert.ok(Number.isFinite(report.fps?.effectiveFps) && report.fps.effectiveFps > 0,
      "effective FPS must be a finite positive number");
    const measurement = productMeasurement(report.exampleId);
    assert.deepEqual(report.measurement, measurement,
    `${name} product measurement protocol changed`);
    if (measurement.gapClock !== null) {
      const fps = sampleRendererFps(report.fpsSamples, measurement.windowEndSeconds, {
        warmupSeconds: measurement.windowStartSeconds,
      });
      assert.deepEqual(report.fps, fps, `${name} camera FPS does not match raw renderer observations`);
      assert.deepEqual(report.presentationGaps, samplePresentationGaps(report.presentationSamples, fps),
        `${name} camera frame gaps do not match raw presentation observations`);
      assert.deepEqual(report.rendererCosts, sampleRendererCosts(report.presentationSamples, report.fpsSamples, fps),
        `${name} renderer costs do not match raw renderer observations`);
    }
    assert.deepEqual(report.packageSizes, summarizePackageSizes(report.packageSizes?.files),
      `${name} package sizes do not match the generated file inventory`);
    assert.match(report.runtimeIdentity?.sourceRevision ?? "", /^[0-9a-f]{40}$/,
      `${name} product source identity is missing`);
    assert.match(report.runtimeIdentity?.buildId ?? "", /^[0-9a-f]{64}$/,
      `${name} product package identity is missing`);
    if (pairCount > 1) {
      assert.equal(report.label, name, "product pair role changed");
      assert.deepEqual(report.pair, { index, position: index === 2 ? 2 - side : side + 1 },
        "product pairs must use the declared alternating order");
      assert.ok(Number.isFinite(report.startedAtMs) && Number.isFinite(report.finishedAtMs) &&
        report.finishedAtMs > report.startedAtMs, "product run interval is invalid");
      if (index > 1) {
        assert.deepEqual(report.runtimeIdentity, pairs[0].reports[side].runtimeIdentity,
          `${name} product package changed between trials`);
        assert.deepEqual(report.packageSizes, pairs[0].reports[side].packageSizes,
          `${name} product package sizes changed between trials`);
        assert.equal(report.exampleId, pairs[0].reports[side].exampleId,
          "product example changed between trials");
        assert.deepEqual(report.runtime, pairs[0].reports[side].runtime,
          "product browser/runtime configuration changed between trials");
      }
    }
  }
  assert.equal(reports[1].exampleId, reports[0].exampleId, "product reports must use the same example");
  assert.equal(reports[1].runtime.backend, reports[0].runtime.backend, "candidate changed the renderer backend");
  assert.deepEqual(reports[1].runtime, reports[0].runtime, "product browser/runtime configuration changed");
  pairs.push({ directories, reports });
}
if (pairCount > 1) {
  const chronological = pairs.flatMap(({ reports }, index) => index === 1 ? [...reports].reverse() : reports);
  assert.ok(chronological.every((report, index) => index === 0 ||
    report.startedAtMs >= chronological[index - 1].finishedAtMs),
  "product paired runs must be serial and retain their declared order");
}
const statistics = [0, 1].map(side => Object.fromEntries([...metricKeys, "fps"].map(key =>
  [key, summarizeSamples(pairs.map(({ reports }) => key === "fps"
    ? reports[side].fps.effectiveFps : reports[side][key]))])));
// Arithmetic means consume every prescribed run; no best-run selection or retry.
const [baseline, candidate] = statistics.map((stats, side) => ({
  ...pairs[0].reports[side],
  ...Object.fromEntries(metricKeys.map(key => [key, stats[key].mean])),
  fps: { effectiveFps: stats.fps.mean },
}));

function cumulativeTrend(anchor, workloadId, reports, measuredStatistics) {
  const workload = anchor.workloads[workloadId];
  const reasons = [];
  const current = reports[0];
  if (pairCount !== 3) reasons.push("current comparison is not the prescribed three-pair cohort");
  if (workload === undefined) reasons.push("anchor has no matching workload");
  if (!isDeepStrictEqual(current.runtime, anchor.runtime)) {
    reasons.push("device, backend, browser, or runtime configuration differs from the anchor");
  }
  if (workload !== undefined) {
    const { version: anchorVersion, ...anchorMeasurement } = workload.measurement;
    const { version: currentVersion, ...currentMeasurement } = current.measurement;
    if (anchorVersion !== anchor.protocolCompatibility.anchorProtocolVersion ||
        currentVersion !== anchor.protocolCompatibility.currentProtocolVersion ||
        !isDeepStrictEqual(anchorMeasurement, currentMeasurement)) {
      reasons.push("measurement windows or protocol semantics differ from the anchor");
    }
  }
  const anchorFps = workload === undefined ? null : summarizeSamples(workload.fpsObservations);
  const anchorLatency = workload === undefined ? null : Object.fromEntries(metricKeys.map(key =>
    [key, summarizeSamples(workload.latencyObservations.map(sample => sample[key]))]));
  const currentLatencyObservations = Object.fromEntries(["parentBaseline", "candidate"].map((side, sideIndex) =>
    [side, pairs.map(pair => Object.fromEntries(metricKeys.map(key =>
      [key, pair.reports[sideIndex][key]])))]));
  const currentLatency = Object.fromEntries(["parentBaseline", "candidate"].map(side =>
    [side, Object.fromEntries(metricKeys.map(key =>
      [key, summarizeSamples(currentLatencyObservations[side].map(sample => sample[key]))]))]));
  const anchorGapObservations = workload?.presentationGapObservations ?? null;
  const currentGapObservations = Object.fromEntries(["parentBaseline", "candidate"].map((side, sideIndex) =>
    [side, pairs.map(pair => pair.reports[sideIndex].presentationGaps ?? null)]));
  const gapKeys = ["min", "p50", "p95", "p99", "max", "mean"];
  const anchorGapsValid = anchorGapObservations === null ||
    (Array.isArray(anchorGapObservations) && anchorGapObservations.length === 3 &&
      anchorGapObservations.every(sample => sample?.clock === workload.measurement.gapClock &&
        gapKeys.every(key => Number.isFinite(sample.intervalMs?.[key]))));
  const currentGapsValid = workload?.measurement.gapClock === null ||
    ["parentBaseline", "candidate"].every(side => currentGapObservations[side].length === 3 &&
      currentGapObservations[side].every(sample => sample?.clock === workload?.measurement.gapClock &&
        gapKeys.every(key => Number.isFinite(sample.intervalMs?.[key]))));
  const frameGapStatus = workload?.measurement.gapClock === null
    ? "not-collected-for-workload"
    : anchorGapsValid && currentGapsValid ? "comparable" : "unavailable";
  const summarizeGaps = observations => observations === null ? null : Object.fromEntries(gapKeys.map(key =>
    [key, summarizeSamples(observations.map(sample => sample.intervalMs[key]))]));
  const anchorGapStatistics = anchorGapsValid ? summarizeGaps(anchorGapObservations) : null;
  const currentGapStatistics = Object.fromEntries(["parentBaseline", "candidate"].map(side =>
    [side, currentGapsValid && workload?.measurement.gapClock !== null
      ? summarizeGaps(currentGapObservations[side]) : null]));
  return {
    policy: "descriptive-only",
    gateApplied: false,
    anchor: {
      id: anchor.anchorId,
      provenance: anchor.provenance,
      runtime: anchor.runtime,
    },
    protocolCompatibility: {
      statement: anchor.protocolCompatibility.statement,
      verifiedEquivalentFields: anchor.protocolCompatibility.verifiedEquivalentFields,
      anchorVersion: anchor.protocolCompatibility.anchorProtocolVersion,
      currentVersion: anchor.protocolCompatibility.currentProtocolVersion,
    },
    workload: workloadId,
    status: reasons.length === 0 ? "comparable" : "not-comparable",
    reasons,
    anchorObservations: workload?.fpsObservations ?? null,
    anchorStatistics: { fps: anchorFps, latency: anchorLatency, presentationGaps: anchorGapStatistics },
    currentIdentities: {
      parentBaseline: reports[0].runtimeIdentity,
      candidate: reports[1].runtimeIdentity,
    },
    currentObservations: {
      parentBaseline: pairs.map(pair => pair.reports[0].fps.effectiveFps),
      candidate: pairs.map(pair => pair.reports[1].fps.effectiveFps),
    },
    latencyObservations: currentLatencyObservations,
    latencyStatistics: currentLatency,
    presentationGapObservations: { anchor: anchorGapObservations, ...currentGapObservations },
    presentationGapStatus: frameGapStatus,
    presentationGapStatistics: { anchor: anchorGapStatistics, ...currentGapStatistics },
    ratios: reasons.length === 0 ? {
      fps: {
        parentBaseline: measuredStatistics[0].fps.mean / anchorFps.mean,
        candidate: measuredStatistics[1].fps.mean / anchorFps.mean,
      },
      latency: Object.fromEntries(["parentBaseline", "candidate"].map(side => [side,
        Object.fromEntries(metricKeys.map(key =>
          [key, currentLatency[side][key].mean / anchorLatency[key].mean]))])),
      presentationGaps: frameGapStatus === "comparable" ? Object.fromEntries(
        ["parentBaseline", "candidate"].map(side => [side,
          Object.fromEntries(gapKeys.map(key => [key,
            currentGapStatistics[side][key].mean / anchorGapStatistics[key].mean]))])) : null,
    } : null,
  };
}
const cumulativeAnchorTrend = cumulativeTrend(cumulativeAnchor, candidate.exampleId,
  pairs[0].reports, statistics);

function threshold(name, fallback) {
  const raw = process.env[name] ?? fallback;
  assert.ok(raw.trim().length > 0, `${name} must not be empty`);
  return Number(raw);
}

const maxLatencyRatio = threshold("NOON_PRODUCT_MAX_LATENCY_RATIO", "1.25");
const latencySlackMs = threshold("NOON_PRODUCT_LATENCY_SLACK_MS", "350");
const minFpsRatio = threshold("NOON_PRODUCT_MIN_FPS_RATIO", "0.80");
const maxVisualDiffRatio = threshold("NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO", "0.015");

assert.ok(Number.isFinite(maxLatencyRatio) && maxLatencyRatio > 0,
  "NOON_PRODUCT_MAX_LATENCY_RATIO must be finite and positive");
assert.ok(Number.isFinite(latencySlackMs) && latencySlackMs >= 0,
  "NOON_PRODUCT_LATENCY_SLACK_MS must be finite and non-negative");
assert.ok(Number.isFinite(minFpsRatio) && minFpsRatio > 0,
  "NOON_PRODUCT_MIN_FPS_RATIO must be finite and positive");
assert.ok(Number.isFinite(maxVisualDiffRatio) && maxVisualDiffRatio >= 0 && maxVisualDiffRatio <= 1,
  "NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO must be between zero and one");

const latency = [
  ["shell ready", baseline.shellReadyMs, candidate.shellReadyMs],
  ["cold Run → applied", baseline.coldRunMs, candidate.coldRunMs],
  ["warm Run → applied", baseline.warmRunMs, candidate.warmRunMs],
  ["edit → applied", baseline.editRunMs, candidate.editRunMs],
];
const failures = [];
for (const [name, before, after] of latency) {
  assert.ok(Number.isFinite(before) && before >= 0 && Number.isFinite(after) && after >= 0,
    `${name}: latency must be finite and non-negative`);
  const limit = before * maxLatencyRatio + latencySlackMs;
  if (after > limit) {
    failures.push(`${name} regressed from ${before.toFixed(0)} ms to ${after.toFixed(0)} ms (limit ${limit.toFixed(0)} ms)`);
  }
}

const baselineFps = baseline.fps?.effectiveFps;
const candidateFps = candidate.fps?.effectiveFps;
assert.ok(
  Number.isFinite(baselineFps) && baselineFps > 0 &&
    Number.isFinite(candidateFps) && candidateFps > 0,
  "effective FPS must be a finite positive number",
);
const fpsFloor = baselineFps * minFpsRatio;
if (candidateFps < fpsFloor) {
  failures.push(
    `effective FPS regressed from ${baselineFps.toFixed(1)} to ${candidateFps.toFixed(1)} (floor ${fpsFloor.toFixed(1)})`,
  );
}

// Reject malformed reports/configuration before decoding any image resources.
const { default: pngjs } = await import("pngjs");
const { PNG } = pngjs;
const visualPairs = [];
for (const [index, { directories, reports }] of pairs.entries()) {
  const images = await Promise.all(directories.map(async (directory, side) =>
    PNG.sync.read(await readFile(path.join(directory, reports[side].screenshot)))));
  const [before, after] = images;
  assert.equal(after.width, before.width, "visual comparison width changed");
  assert.equal(after.height, before.height, "visual comparison height changed");
  let differingPixels = 0;
  for (let offset = 0; offset < before.data.length; offset += 4) {
    const distance = Math.abs(before.data[offset] - after.data[offset]) +
      Math.abs(before.data[offset + 1] - after.data[offset + 1]) +
      Math.abs(before.data[offset + 2] - after.data[offset + 2]) +
      Math.abs(before.data[offset + 3] - after.data[offset + 3]);
    if (distance >= 32) differingPixels += 1;
  }
  const pixelCount = before.width * before.height;
  const diffRatio = differingPixels / pixelCount;
  visualPairs.push({ pair: index + 1, differingPixels, pixelCount, diffRatio });
  if (diffRatio > maxVisualDiffRatio) {
    failures.push(`pair ${index + 1} deterministic frame visual diff is ${(diffRatio * 100).toFixed(2)}% ` +
      `(${differingPixels}/${pixelCount}), limit ${(maxVisualDiffRatio * 100).toFixed(2)}%`);
  }
}
const visualDiffRatio = Math.max(...visualPairs.map(pair => pair.diffRatio));

const costStatistics = pairs[0].reports[0].measurement.gapClock === null ? null
  : Object.fromEntries(["baseline", "candidate"].map((side, index) => [side, {
    cpuWallMs: Object.fromEntries(["applyMs", "renderMs", "ackPostMs"].map(key =>
      [key, summarizeSamples(pairs.map(({ reports }) => reports[index].rendererCosts.cpuWallMs[key].mean))])),
    sampledBytesUploaded: summarizeSamples(pairs.map(({ reports }) =>
      reports[index].rendererCosts.sampledLastFrame.fields.bytesUploaded.mean)),
  }]));

const comparison = {
  schemaVersion: 2,
  protocol: { pairs: pairCount, aggregate: "arithmetic-mean", order: pairs.map((_, index) =>
    index === 1 ? ["candidate", "baseline"] : ["baseline", "candidate"]) },
  measurements: pairs.map(({ directories, reports }) => ({ directories,
    baseline: reports[0], candidate: reports[1] })),
  statistics: { baseline: statistics[0], candidate: statistics[1] },
  exampleId: candidate.exampleId,
  thresholds: { maxLatencyRatio, latencySlackMs, minFpsRatio, maxVisualDiffRatio },
  latency: Object.fromEntries(
    latency.map(([name, before, after]) => [name, { baselineMs: before, candidateMs: after }]),
  ),
  fps: { baseline: baselineFps, candidate: candidateFps, floor: fpsFloor },
  // Shared software-GPU frame gaps are attribution evidence, not a physical
  // display budget. Every run is retained beside the unchanged FPS floor.
  presentationGaps: pairs.map(({ reports }, index) => ({ pair: index + 1,
    baseline: reports[0].presentationGaps ?? null, candidate: reports[1].presentationGaps ?? null })),
  // Descriptive costs retain all runs; they do not invent additional noisy thresholds.
  rendererCosts: pairs.map(({ reports }, index) => ({ pair: index + 1,
    baseline: reports[0].rendererCosts ?? null, candidate: reports[1].rendererCosts ?? null })),
  costStatistics,
  packageSizes: { baseline: baseline.packageSizes, candidate: candidate.packageSizes },
  cumulativeAnchorTrend,
  visual: { pairs: visualPairs, worstDiffRatio: visualDiffRatio },
  failures,
};
await writeFile(
  path.join(candidateDir, "comparison.json"),
  `${JSON.stringify(comparison, null, 2)}\n`,
  "utf8",
);

console.log(`Compared ${pairCount} prescribed pair(s), arithmetic means; FPS ranges ` +
  `${statistics[0].fps.min.toFixed(1)}–${statistics[0].fps.max.toFixed(1)} / ` +
  `${statistics[1].fps.min.toFixed(1)}–${statistics[1].fps.max.toFixed(1)}.`);
console.log("| Product metric | Baseline | Candidate |");
console.log("| --- | ---: | ---: |");
for (const [name, before, after] of latency) {
  console.log(`| ${name} | ${before.toFixed(0)} ms | ${after.toFixed(0)} ms |`);
}
console.log(`| Effective FPS | ${baselineFps.toFixed(1)} | ${candidateFps.toFixed(1)} |`);
if (cumulativeAnchorTrend.status === "comparable") {
  console.log(`| FPS ratio to pinned ${cumulativeAnchor.anchorId} | ` +
    `${cumulativeAnchorTrend.ratios.fps.parentBaseline.toFixed(3)} | ` +
    `${cumulativeAnchorTrend.ratios.fps.candidate.toFixed(3)} |`);
  if (cumulativeAnchorTrend.presentationGapStatus === "comparable") {
    console.log(`| Renderer frame-gap p95 ratio to pinned ${cumulativeAnchor.anchorId} | ` +
      `${cumulativeAnchorTrend.ratios.presentationGaps.parentBaseline.p95.toFixed(3)} | ` +
      `${cumulativeAnchorTrend.ratios.presentationGaps.candidate.p95.toFixed(3)} |`);
  }
} else {
  console.log(`Cumulative anchor trend not comparable: ${cumulativeAnchorTrend.reasons.join("; ")}`);
}
if (costStatistics !== null) {
  for (const [key, label] of [["applyMs", "Delta apply CPU wall"], ["renderMs", "Render call CPU wall"],
    ["ackPostMs", "Acknowledgment post CPU wall"]]) {
    console.log(`| ${label} (mean) | ${costStatistics.baseline.cpuWallMs[key].mean.toFixed(3)} ms | ` +
      `${costStatistics.candidate.cpuWallMs[key].mean.toFixed(3)} ms |`);
  }
  console.log(`| Sampled last-frame upload (mean) | ${costStatistics.baseline.sampledBytesUploaded.mean.toFixed(0)} bytes | ` +
    `${costStatistics.candidate.sampledBytesUploaded.mean.toFixed(0)} bytes |`);
}
console.log(`| Generated package (uncompressed) | ${baseline.packageSizes.totalBytes} bytes | ${candidate.packageSizes.totalBytes} bytes |`);
console.log(`| Fixed-frame pixel diff | — | ${(visualDiffRatio * 100).toFixed(2)}% |`);

if (failures.length > 0) {
  for (const failure of failures) console.error(`REGRESSION: ${failure}`);
  process.exitCode = 2;
} else {
  console.log("Product regression comparison passed");
}
