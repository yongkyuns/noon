import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import pngjs from "pngjs";

const { PNG } = pngjs;
const [baselineDirArg, candidateDirArg] = process.argv.slice(2);
assert.ok(
  baselineDirArg && candidateDirArg,
  "usage: node scripts/playground-product-compare.mjs BASELINE_DIR CANDIDATE_DIR",
);
const baselineDir = path.resolve(baselineDirArg);
const candidateDir = path.resolve(candidateDirArg);
const baseline = JSON.parse(await readFile(path.join(baselineDir, "report.json"), "utf8"));
const candidate = JSON.parse(await readFile(path.join(candidateDir, "report.json"), "utf8"));

assert.equal(candidate.exampleId, baseline.exampleId, "product reports must use the same example");
assert.equal(candidate.runtime.backend, baseline.runtime.backend, "candidate changed the renderer backend");

const maxLatencyRatio = Number(process.env.NOON_PRODUCT_MAX_LATENCY_RATIO ?? "1.25");
const latencySlackMs = Number(process.env.NOON_PRODUCT_LATENCY_SLACK_MS ?? "350");
const minFpsRatio = Number(process.env.NOON_PRODUCT_MIN_FPS_RATIO ?? "0.80");
const maxVisualDiffRatio = Number(process.env.NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO ?? "0.015");

const latency = [
  ["shell ready", baseline.shellReadyMs, candidate.shellReadyMs],
  ["cold Run → applied", baseline.coldRunMs, candidate.coldRunMs],
  ["warm Run → applied", baseline.warmRunMs, candidate.warmRunMs],
  ["edit → applied", baseline.editRunMs, candidate.editRunMs],
];
const failures = [];
for (const [name, before, after] of latency) {
  assert.ok(Number.isFinite(before) && Number.isFinite(after), `${name}: non-finite latency`);
  const limit = before * maxLatencyRatio + latencySlackMs;
  if (after > limit) {
    failures.push(`${name} regressed from ${before.toFixed(0)} ms to ${after.toFixed(0)} ms (limit ${limit.toFixed(0)} ms)`);
  }
}

const baselineFps = Number(baseline.fps?.effectiveFps);
const candidateFps = Number(candidate.fps?.effectiveFps);
assert.ok(Number.isFinite(baselineFps) && Number.isFinite(candidateFps), "effective FPS must be finite");
const fpsFloor = baselineFps * minFpsRatio;
if (candidateFps < fpsFloor) {
  failures.push(
    `effective FPS regressed from ${baselineFps.toFixed(1)} to ${candidateFps.toFixed(1)} (floor ${fpsFloor.toFixed(1)})`,
  );
}

const baselineImage = PNG.sync.read(await readFile(path.join(baselineDir, baseline.screenshot)));
const candidateImage = PNG.sync.read(await readFile(path.join(candidateDir, candidate.screenshot)));
assert.equal(candidateImage.width, baselineImage.width, "visual comparison width changed");
assert.equal(candidateImage.height, baselineImage.height, "visual comparison height changed");
let differingPixels = 0;
for (let offset = 0; offset < baselineImage.data.length; offset += 4) {
  const distance =
    Math.abs(baselineImage.data[offset] - candidateImage.data[offset]) +
    Math.abs(baselineImage.data[offset + 1] - candidateImage.data[offset + 1]) +
    Math.abs(baselineImage.data[offset + 2] - candidateImage.data[offset + 2]) +
    Math.abs(baselineImage.data[offset + 3] - candidateImage.data[offset + 3]);
  if (distance >= 32) differingPixels += 1;
}
const pixelCount = baselineImage.width * baselineImage.height;
const visualDiffRatio = differingPixels / pixelCount;
if (visualDiffRatio > maxVisualDiffRatio) {
  failures.push(
    `deterministic frame visual diff is ${(visualDiffRatio * 100).toFixed(2)}% (${differingPixels}/${pixelCount}), ` +
      `limit ${(maxVisualDiffRatio * 100).toFixed(2)}%`,
  );
}

const comparison = {
  schemaVersion: 1,
  exampleId: candidate.exampleId,
  thresholds: { maxLatencyRatio, latencySlackMs, minFpsRatio, maxVisualDiffRatio },
  latency: Object.fromEntries(
    latency.map(([name, before, after]) => [name, { baselineMs: before, candidateMs: after }]),
  ),
  fps: { baseline: baselineFps, candidate: candidateFps, floor: fpsFloor },
  visual: { differingPixels, pixelCount, diffRatio: visualDiffRatio },
  failures,
};
await writeFile(
  path.join(candidateDir, "comparison.json"),
  `${JSON.stringify(comparison, null, 2)}\n`,
  "utf8",
);

console.log("| Product metric | Baseline | Candidate |");
console.log("| --- | ---: | ---: |");
for (const [name, before, after] of latency) {
  console.log(`| ${name} | ${before.toFixed(0)} ms | ${after.toFixed(0)} ms |`);
}
console.log(`| Effective FPS | ${baselineFps.toFixed(1)} | ${candidateFps.toFixed(1)} |`);
console.log(`| Fixed-frame pixel diff | — | ${(visualDiffRatio * 100).toFixed(2)}% |`);

if (failures.length > 0) {
  for (const failure of failures) console.error(`REGRESSION: ${failure}`);
  process.exitCode = 2;
} else {
  console.log("Product regression comparison passed");
}
