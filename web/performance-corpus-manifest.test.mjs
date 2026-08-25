import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(
  await readFile(path.join(repoRoot, "benchmarks/performance-scenes.json"), "utf8"),
);
const allCases = [...manifest.cases, ...(manifest.auxiliaryCases ?? [])];
const byId = new Map(allCases.map((definition) => [definition.id, definition]));
const domains = Array.from({ length: 7 }, (_, index) => `P${index + 1}`);

function sourcePath(definition) {
  return definition.source.startsWith("./")
    ? path.join(repoRoot, "web", definition.source.slice(2))
    : path.join(repoRoot, definition.source);
}

test("performance corpus sources and classifications are explicit", async () => {
  assert.equal(manifest.schemaVersion, 1);
  assert.equal(byId.size, allCases.length, "performance case IDs must be unique");
  const allowed = new Set(["representative", "adversarial", "scalability-only"]);
  for (const definition of allCases) {
    assert.ok(allowed.has(definition.classification), `${definition.id}: classification`);
    assert.ok(Array.isArray(definition.domains) && definition.domains.length > 0, `${definition.id}: domains`);
    assert.ok(Array.isArray(definition.dimensions) && definition.dimensions.length > 0, `${definition.id}: dimensions`);
    await access(sourcePath(definition));
  }
});

test("every P1-P7 domain has a realistic representative", () => {
  for (const domain of domains) {
    const ids = manifest.domainCoverage?.[domain];
    assert.ok(Array.isArray(ids) && ids.length > 0, `${domain}: missing coverage declaration`);
    const definitions = ids.map((id) => {
      const definition = byId.get(id);
      assert.ok(definition, `${domain}: unknown case ${id}`);
      assert.ok(definition.domains.includes(domain), `${domain}: ${id} does not declare the domain`);
      return definition;
    });
    assert.ok(
      definitions.some(({ classification }) => classification === "representative"),
      `${domain}: requires at least one representative scene`,
    );
  }
});

test("60 Hz budgets and 120 Hz diagnostic policy stay explicit", () => {
  const common = manifest.tiers.interactive60.budgets;
  assert.ok(common.frameIntervalP95Ms > 0);
  assert.ok(common.frameIntervalP99Ms >= common.frameIntervalP95Ms);
  assert.ok(common.longFrameRateMax >= 0 && common.longFrameRateMax < 1);
  assert.equal(manifest.reportingPolicy.referenceTargetHz, 60);
  assert.equal(manifest.reportingPolicy.diagnosticTargetHz, 120);
  assert.equal(manifest.reportingPolicy.diagnostic120IsGate, false);
  assert.equal(manifest.reportingPolicy.sharedRunnerWallClockGate, false);
});

test("anti-gaming and host fallback cases remain in the corpus", () => {
  const overlap = byId.get("painter-order-overlap");
  assert.ok(overlap?.dimensions.includes("mixed-transparency"));
  assert.ok(overlap?.dimensions.includes("fixed-pixel-footprint"));
  const host = byId.get("host-updater-follow");
  assert.equal(host?.classification, "representative");
  assert.ok(host?.domains.includes("P5"));
  assert.match(host?.runner ?? "", /updater-callback-smoke/);
});
