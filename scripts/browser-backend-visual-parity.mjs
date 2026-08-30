import assert from "node:assert/strict";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";

import pngjs from "pngjs";

import {
  compareForegroundCoverage,
  foregroundMismatchMask,
} from "./browser-visual-parity-lib.mjs";

const { PNG } = pngjs;

const [webgpuDir, webglDir, outputDir] = process.argv.slice(2);
if (!webgpuDir || !webglDir || !outputDir) {
  throw new Error(
    "usage: node scripts/browser-backend-visual-parity.mjs <webgpu-dir> <webgl-dir> <output-dir>",
  );
}

function finiteEnv(name, fallback) {
  const raw = process.env[name];
  if (raw === undefined) {
    return fallback;
  }
  const value = Number(raw);
  if (!Number.isFinite(value)) {
    throw new Error(`${name} must be finite, got ${raw}`);
  }
  return value;
}

const tolerances = {
  backgroundDistance: finiteEnv("NOON_BACKEND_PARITY_BACKGROUND_DISTANCE", 32),
  neighborRadius: finiteEnv("NOON_BACKEND_PARITY_NEIGHBOR_RADIUS", 1),
  maxMismatchFraction: finiteEnv("NOON_BACKEND_PARITY_MAX_MISMATCH_FRACTION", 0.02),
  maxBoundsDelta: finiteEnv("NOON_BACKEND_PARITY_MAX_BOUNDS_DELTA", 2),
};
assert.ok(Number.isInteger(tolerances.neighborRadius) && tolerances.neighborRadius >= 0);

async function pngNames(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".png"))
    .map((entry) => entry.name)
    .sort();
}

function mismatchPng(width, height, mask) {
  const diff = new PNG({ width, height });
  for (let pixel = 0; pixel < mask.length; pixel += 1) {
    const offset = pixel * 4;
    const value = mask[pixel] === 0 ? 0 : 255;
    diff.data[offset] = value;
    diff.data[offset + 1] = value;
    diff.data[offset + 2] = value;
    diff.data[offset + 3] = mask[pixel] === 0 ? 0 : 255;
  }
  return PNG.sync.write(diff);
}

await mkdir(outputDir, { recursive: true });

const webgpuNames = await pngNames(webgpuDir);
const webglNames = await pngNames(webglDir);
assert.ok(webgpuNames.length > 0, `no browser-smoke PNGs found in ${webgpuDir}`);
assert.deepEqual(
  webglNames,
  webgpuNames,
  "WebGPU and WebGL browser-smoke artifact sets must contain the same deterministic checkpoints",
);

const results = [];
for (const name of webgpuNames) {
  const [webgpuBuffer, webglBuffer] = await Promise.all([
    readFile(path.join(webgpuDir, name)),
    readFile(path.join(webglDir, name)),
  ]);
  const webgpu = PNG.sync.read(webgpuBuffer);
  const webgl = PNG.sync.read(webglBuffer);
  const comparison = compareForegroundCoverage(webgpu, webgl, tolerances);
  results.push({ name, ...comparison });

  const marker = comparison.pass ? "✓" : "✗";
  console.log(
    `${marker} ${name}: ${(comparison.mismatchFraction * 100).toFixed(3)}% unmatched foreground, ` +
      `${comparison.boundsDelta}px bounds delta`,
  );

  if (!comparison.pass) {
    const mask = foregroundMismatchMask(webgpu, webgl, tolerances);
    await writeFile(
      path.join(outputDir, name.replace(/\.png$/u, "-coverage-diff.png")),
      mismatchPng(webgpu.width, webgpu.height, mask),
    );
  }
}

const failed = results.filter((result) => !result.pass);
const report = {
  schemaVersion: 1,
  comparison: "foreground-coverage",
  rationale:
    "Compare renderer geometry/coverage while deliberately ignoring backend color-transfer differences tracked separately.",
  tolerances,
  compared: results.length,
  failed: failed.length,
  results,
};
await writeFile(
  path.join(outputDir, "browser-backend-visual-parity.json"),
  `${JSON.stringify(report, null, 2)}\n`,
);

assert.deepEqual(
  failed.map((result) => result.name),
  [],
  `WebGPU/WebGL foreground coverage diverged for ${failed.length}/${results.length} checkpoints; see ${outputDir}`,
);

console.log(
  `Browser backend visual parity passed for ${results.length} deterministic screenshots ` +
    `(<= ${(tolerances.maxMismatchFraction * 100).toFixed(2)}% unmatched foreground, ` +
    `<= ${tolerances.maxBoundsDelta}px bounds delta).`,
);
