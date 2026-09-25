// External assertions over real reference/worker PNGs. No scene-side pixel model.
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const CASES = [
  ["matching-ordinary-foreground", "MatchingOrdinaryForeground"],
  ["matching-foreground-source", "MatchingForegroundSource"],
  ["matching-foreground-only-layer", "MatchingForegroundOnlyLayer"],
];
// Match the ordered f64 additions in play(2), wait(0.2), wait(0.2).
// The decimal literal 2.4 is one ULP before the shared execution endpoint.
export const DURATION = 2 + 0.2 + 0.2;
export const TIMES = [0, 0.5, 1, 1.5, 2, 2.2];
const BACKENDS = ["webgpu", "webgl"];

export function validateReport(manifest, report) {
  assert.equal(manifest.reference.version, "0.21.0");
  assert.equal(manifest.reference.renderer, "cairo");
  assert.equal(manifest.reference.frame_rate, 30);
  assert.equal(manifest.reference.pixel_width, 960);
  assert.equal(manifest.reference.pixel_height, 540);
  assert.deepEqual(report.reference, manifest.reference, "reference provenance changed");
  assert.deepEqual(manifest.fixtures.map(f => [f.id, f.scene]), CASES, "corpus changed");
  assert.deepEqual(report.fixtures.map(f => [f.id, f.scene]), CASES, "incomplete report");
  for (const [index, fixture] of report.fixtures.entries()) {
    assert.equal(manifest.fixtures[index].expected_duration, DURATION);
    assert.deepEqual(manifest.fixtures[index].sample_times, TIMES);
    assert.equal(fixture.expectedDuration, DURATION);
    assert.deepEqual(Object.keys(fixture.backends).sort(), [...BACKENDS].sort(), "missing backend");
    for (const backend of BACKENDS) {
      const entry = fixture.backends[backend];
      assert.ok(!entry.error, `${fixture.id}/${backend}: ${entry.error}`);
      assert.equal(entry.noonDuration, DURATION, "source did not complete at its exact endpoint");
      assert.equal(entry.durationDelta, 0, "logical duration differs");
      assert.equal(entry.samples.length, TIMES.length, "missing/extra samples");
      assert.equal(new Set(entry.samples.map(s => s.frameIndex)).size, TIMES.length, "duplicate frames");
      for (const [i, sample] of entry.samples.entries()) {
        assert.ok(Number.isSafeInteger(sample.frameIndex) && sample.frameIndex >= 0);
        assert.ok(Math.abs(sample.time - TIMES[i]) < 1e-9, "wrong sample time");
        assert.equal(sample.debugFrame?.engine, "noon", "missing effective-frame evidence");
        assert.ok(Math.abs(sample.debugFrame.time - sample.time) < 1e-9, "stale effective frame");
      }
    }
    assert.deepEqual(fixture.backends.webgpu.samples.map(s => [s.frameIndex, s.time]),
      fixture.backends.webgl.samples.map(s => [s.frameIndex, s.time]), "backend sample maps differ");
  }
}

// Check 3x3 patches safely inside geometry, not antialiased edges. The 8-unit
// camera height is the pinned default used by both ordinary fixture hosts.
export function assertPatch(png, x, y, predicate, label) {
  const { width, height, data } = png;
  assert.equal(data.length, width * height * 4, "invalid RGBA buffer");
  const px = Math.round(width / 2 + x * height / 8);
  const py = Math.round(height / 2 - y * height / 8);
  assert.ok(px > 0 && px < width - 1 && py > 0 && py < height - 1, "witness outside image");
  for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) {
    const offset = ((py + dy) * width + px + dx) * 4;
    const rgba = [...data.subarray(offset, offset + 4)];
    assert.ok(rgba[3] === 255 && predicate(...rgba), `${label}: ${rgba} at ${px + dx},${py + dy}`);
  }
}

export function assertWitnesses(png, id, time) {
  assert.ok(CASES.some(([candidate]) => candidate === id), "unknown foreground case");
  assert.ok(TIMES.some(t => Math.abs(t - time) < 1e-9), "unknown witness time");
  assert.equal(png.width, 960);
  assert.equal(png.height, 540);
  const white = (r, g, b) => Math.min(r, g, b) >= 250;
  const red = (r, g, b) => r >= 250 && g <= 5 && b <= 5;
  const green = (r, g, b) => r <= 5 && g >= 250 && b <= 5;
  const matchingColor = (r, g, b) => r <= 5 &&
    Math.abs(g - 255 * time / 2) <= 3 && Math.abs(b - 255 * (1 - time / 2)) <= 3;
  const foregroundSource = id === "matching-foreground-source" && time < 2;
  assertPatch(png, -2.4, 0, foregroundSource ? matchingColor : white,
    `${id}@${time}: matched-source/foreground order`);
  assertPatch(png, 2.4, 0, white, `${id}@${time}: target-only occurrence covered by foreground`);
  if (time < 2) {
    assertPatch(png, -2.4, -0.4, matchingColor, `${id}@${time}: real interpolation outside foreground`);
  } else if (time < 2.2 - 1e-9) {
    assertPatch(png, 0.8, -0.4, green, `${id}@${time}: padded target survives exact completion`);
  }
  if (time >= 0.5 && time < 2.2 - 1e-9) {
    assertPatch(png, 2.4, -0.4, (r, g, b) => r <= 5 && g > 30 && b <= 5,
      `${id}@${time}: target-only shape actually exists outside foreground`);
  }
  if (time >= 2.2 - 1e-9) {
    assertPatch(png, 0, -0.35, red, `${id}@${time}: later ordinary addition is visible`);
  }
  assertPatch(png, 0, 3, (r, g, b) => r === 0 && g === 0 && b === 0,
    `${id}@${time}: untouched background`);
}

async function main() {
  const { readFile, writeFile } = await import("node:fs/promises");
  const { PNG } = await import("pngjs");
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const manifestPath = path.resolve(root, process.env.NOON_MANIM_RASTER_MANIFEST ??
    "parity/manim-v0.21/foreground-matching-manifest.json");
  const output = path.resolve(root, process.env.NOON_MANIM_RASTER_ARTIFACTS ?? "foreground-matching-artifacts");
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  const report = JSON.parse(await readFile(path.join(output, "report.json"), "utf8"));
  validateReport(manifest, report);
  const results = [];
  const failures = [];
  for (const fixture of report.fixtures) {
    for (const backend of BACKENDS) for (const sample of fixture.backends[backend].samples) {
      const label = `frame-${String(sample.frameIndex).padStart(4, "0")}.png`;
      for (const engine of ["reference", backend]) {
        try {
          const png = PNG.sync.read(await readFile(path.join(output, engine, fixture.id, label)));
          assertWitnesses(png, fixture.id, sample.time);
          results.push({ id: fixture.id, engine, time: sample.time, passed: true });
        } catch (error) {
          const detail = `${fixture.id}/${engine}/${label}: ${error.stack ?? error}`;
          failures.push(detail);
          results.push({ id: fixture.id, engine, time: sample.time, error: detail });
        }
      }
    }
  }
  await writeFile(path.join(output, "foreground-witness-report.json"),
    `${JSON.stringify({ reference: report.reference, results, failures }, null, 2)}\n`);
  assert.equal(failures.length, 0, failures.join("\n"));
  console.log(`Foreground overlap witnesses passed: ${results.length} image checks (reference checked per backend).`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
