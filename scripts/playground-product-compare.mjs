import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { summarizeSamples } from "../web/frame-metrics.js";
import { productMeasurement, sampleRendererFps, samplePresentationGaps, sampleRendererCosts } from "./playground-product-fps.mjs";
import { summarizePackageSizes } from "../.github/ci/wasm-build.mjs";
import { pairedCostConfidence, pairedThroughput } from "./paired-product-metrics.mjs";

const [baselineDirArg, candidateDirArg, mode, count] = process.argv.slice(2);
const pairCount = mode === undefined ? 1 : Number(count);
assert.ok(baselineDirArg && candidateDirArg &&
  (mode === undefined || (mode === "--pairs" && [3, 7].includes(pairCount))) && process.argv.length <= 6,
  "usage: node scripts/playground-product-compare.mjs BASELINE_DIR CANDIDATE_DIR [--pairs 3|7]");
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
      assert.deepEqual(report.pair, { index, position: index % 2 === 0 ? 2 - side : side + 1 },
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
  const chronological = pairs.flatMap(({ reports }, index) => index % 2 === 1 ? [...reports].reverse() : reports);
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

function threshold(name, fallback) {
  const raw = process.env[name] ?? fallback;
  assert.ok(raw.trim().length > 0, `${name} must not be empty`);
  return Number(raw);
}

const maxLatencyRatio = threshold("NOON_PRODUCT_MAX_LATENCY_RATIO", "1.25");
const latencySlackMs = threshold("NOON_PRODUCT_LATENCY_SLACK_MS", "350");
const minFpsRatio = threshold("NOON_PRODUCT_MIN_FPS_RATIO", "0.80");
const strictMinFpsRatio = threshold("NOON_PRODUCT_STRICT_MIN_FPS_RATIO", "0.97");
const maxVisualDiffRatio = threshold("NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO", "0.015");

assert.ok(Number.isFinite(maxLatencyRatio) && maxLatencyRatio > 0,
  "NOON_PRODUCT_MAX_LATENCY_RATIO must be finite and positive");
assert.ok(Number.isFinite(latencySlackMs) && latencySlackMs >= 0,
  "NOON_PRODUCT_LATENCY_SLACK_MS must be finite and non-negative");
assert.ok(Number.isFinite(minFpsRatio) && minFpsRatio > 0,
  "NOON_PRODUCT_MIN_FPS_RATIO must be finite and positive");
assert.ok(Number.isFinite(strictMinFpsRatio) && strictMinFpsRatio > 0 && strictMinFpsRatio <= 1,
  "NOON_PRODUCT_STRICT_MIN_FPS_RATIO must be in (0, 1]");
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
const fpsPaired = pairCount > 1 ? pairedThroughput(
  pairs.map(({ reports }) => reports[0].fps.effectiveFps),
  pairs.map(({ reports }) => reports[1].fps.effectiveFps),
  strictMinFpsRatio,
) : null;
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
const renderCostPaired = pairCount > 1 && costStatistics !== null ? pairedCostConfidence(
  pairs.map(({ reports }) => reports[0].rendererCosts.cpuWallMs.renderMs.mean),
  pairs.map(({ reports }) => reports[1].rendererCosts.cpuWallMs.renderMs.mean),
) : null;

const comparison = {
  schemaVersion: 2,
  protocol: { pairs: pairCount, aggregate: "arithmetic-mean", order: pairs.map((_, index) =>
    index === 1 ? ["candidate", "baseline"] : ["baseline", "candidate"]) },
  measurements: pairs.map(({ directories, reports }) => ({ directories,
    baseline: reports[0], candidate: reports[1] })),
  statistics: { baseline: statistics[0], candidate: statistics[1] },
  exampleId: candidate.exampleId,
  thresholds: { maxLatencyRatio, latencySlackMs, minFpsRatio, strictMinFpsRatio, maxVisualDiffRatio },
  latency: Object.fromEntries(
    latency.map(([name, before, after]) => [name, { baselineMs: before, candidateMs: after }]),
  ),
  fps: { baseline: baselineFps, candidate: candidateFps, floor: fpsFloor, paired: fpsPaired },
  rendererCostQualification: renderCostPaired === null ? null : { renderMs: renderCostPaired },
  // Shared software-GPU frame gaps are attribution evidence, not a physical
  // display budget. Every run is retained beside the unchanged FPS floor.
  presentationGaps: pairs.map(({ reports }, index) => ({ pair: index + 1,
    baseline: reports[0].presentationGaps ?? null, candidate: reports[1].presentationGaps ?? null })),
  // Descriptive costs retain all runs; they do not invent additional noisy thresholds.
  rendererCosts: pairs.map(({ reports }, index) => ({ pair: index + 1,
    baseline: reports[0].rendererCosts ?? null, candidate: reports[1].rendererCosts ?? null })),
  costStatistics,
  packageSizes: { baseline: baseline.packageSizes, candidate: candidate.packageSizes },
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
if (fpsPaired !== null) {
  console.log(`Paired FPS ratio: ${fpsPaired.ratio.toFixed(4)}x [${fpsPaired.lower.toFixed(4)}, ${fpsPaired.upper.toFixed(4)}] ${fpsPaired.status}`);
}
if (renderCostPaired !== null) {
  console.log(`Paired render CPU cost: ${renderCostPaired.ratio.toFixed(4)}x [${renderCostPaired.lower.toFixed(4)}, ${renderCostPaired.upper.toFixed(4)}] ${renderCostPaired.status}`);
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
