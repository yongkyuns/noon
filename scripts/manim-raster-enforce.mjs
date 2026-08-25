import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  evaluateRasterTolerance,
  formatRasterPolicyFailure,
  resolveRasterTolerance,
} from "./manim-raster-policy.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const artifactRoot = path.resolve(
  repoRoot,
  process.env.NOON_MANIM_RASTER_ARTIFACTS ?? "manim-raster-artifacts",
);
const manifest = JSON.parse(
  await readFile(path.join(repoRoot, "parity", "manim-v0.21", "manifest.json"), "utf8"),
);
const report = JSON.parse(await readFile(path.join(artifactRoot, "report.json"), "utf8"));

assert.equal(report.reference.version, manifest.reference.version, "ratchet/report Manim version");
assert.equal(report.reference.renderer, manifest.reference.renderer, "ratchet/report renderer");

const manifestFixtures = new Map(manifest.fixtures.map((fixture) => [fixture.id, fixture]));
const failures = [];
const fixtureResults = [];

for (const fixtureReport of report.fixtures) {
  const fixture = manifestFixtures.get(fixtureReport.id);
  assert.ok(fixture, `${fixtureReport.id}: report fixture must exist in manifest`);
  const tolerance = resolveRasterTolerance(manifest, fixture);
  const backendResults = {};

  for (const [backend, backendReport] of Object.entries(fixtureReport.backends)) {
    const samples = [];
    for (const sample of backendReport.samples) {
      const result = evaluateRasterTolerance({
        sample,
        timingDelta: backendReport.durationDelta,
        tolerance,
      });
      samples.push({
        frameIndex: sample.frameIndex,
        time: sample.time,
        passed: result.passed,
        categories: result.categories,
        failures: result.failures,
      });
      if (!result.passed) {
        failures.push(
          formatRasterPolicyFailure(
            fixtureReport.id,
            backend,
            `frame-${String(sample.frameIndex).padStart(4, "0")}`,
            result,
          ),
        );
      }
    }
    backendResults[backend] = {
      durationDelta: backendReport.durationDelta,
      samples,
    };
  }

  fixtureResults.push({ id: fixtureReport.id, tolerance, backends: backendResults });
}

for (const fixture of manifest.fixtures) {
  assert.ok(
    report.fixtures.some((entry) => entry.id === fixture.id),
    `${fixture.id}: manifest fixture missing from raster report`,
  );
}

const ratchetReport = {
  generatedAt: new Date().toISOString(),
  sourceReportEnforceFlag: report.enforce,
  passed: failures.length === 0,
  failures,
  fixtures: fixtureResults,
};
await writeFile(
  path.join(artifactRoot, "ratchet-report.json"),
  `${JSON.stringify(ratchetReport, null, 2)}\n`,
);

if (failures.length > 0) {
  throw new Error(`Manim raster ratchet failures:\n${failures.join("\n")}`);
}
console.log(`Manim raster ratchet passed for ${fixtureResults.length} fixtures`);
