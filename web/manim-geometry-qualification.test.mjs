import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const qualificationUrl = new URL(
  "../parity/manim-v0.21/geometry-qualification.json",
  import.meta.url,
);
const rasterManifestUrl = new URL(
  "../parity/manim-v0.21/manifest.json",
  import.meta.url,
);
const sourceUrl = new URL("../parity/manim-v0.21/quickstart.py", import.meta.url);
const coverageUrl = new URL("../compat/manim-v0.21.0.json", import.meta.url);

async function readJson(url) {
  return JSON.parse(await readFile(url, "utf8"));
}

test("qualified geometry is backed by a canonical raster/timeline fixture", async () => {
  const [qualification, rasterManifest, coverage, source] = await Promise.all([
    readJson(qualificationUrl),
    readJson(rasterManifestUrl),
    readJson(coverageUrl),
    readFile(sourceUrl, "utf8"),
  ]);

  assert.equal(qualification.reference.project, "Manim Community");
  assert.equal(qualification.reference.version, "0.21.0");
  assert.equal(qualification.reference.renderer, "cairo");

  const fixtures = new Map(
    rasterManifest.fixtures.map((fixture) => [fixture.id, fixture]),
  );
  const seenSymbols = new Set();

  for (const entry of qualification.qualified) {
    assert.equal(
      seenSymbols.has(entry.symbol),
      false,
      `duplicate geometry qualification for ${entry.symbol}`,
    );
    seenSymbols.add(entry.symbol);

    const publicStatus = coverage.overrides[entry.symbol];
    assert.ok(publicStatus, `missing compatibility entry for ${entry.symbol}`);
    assert.equal(publicStatus.category, "geometry");
    assert.equal(
      publicStatus.status,
      "supported",
      `${entry.symbol} cannot be exact-output-qualified before it is public and supported`,
    );

    const fixture = fixtures.get(entry.fixture);
    assert.ok(
      fixture,
      `${entry.symbol} qualification references unknown fixture ${entry.fixture}`,
    );
    assert.ok(
      Number.isFinite(fixture.expected_duration) && fixture.expected_duration > 0,
      `${entry.fixture} must participate in the timeline oracle`,
    );
    assert.match(
      source,
      new RegExp(`class\\s+${fixture.scene}\\s*\\(Scene\\):`),
      `${entry.fixture} must resolve to canonical Manim source`,
    );
  }
});

test("canonical geometry support claims are recorded in the qualification ledger", async () => {
  const [qualification, coverage] = await Promise.all([
    readJson(qualificationUrl),
    readJson(coverageUrl),
  ]);
  const qualified = new Set(qualification.qualified.map((entry) => entry.symbol));

  for (const [symbol, status] of Object.entries(coverage.overrides)) {
    if (
      status.category === "geometry" &&
      status.status === "supported" &&
      String(status.evidence ?? "").toLowerCase().includes("canonical")
    ) {
      assert.ok(
        qualified.has(symbol),
        `${symbol} claims canonical exact-output evidence but has no #401 qualification record`,
      );
    }
  }
});
