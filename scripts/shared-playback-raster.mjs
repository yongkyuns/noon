import assert from "node:assert/strict";
import { createReadStream } from "node:fs";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import pngjs from "pngjs";
import { browserArgs, rasterFixtureSource, compareEffectiveFrames, MAX_EFFECTIVE_ABSOLUTE_ERROR } from "./manim-raster-support.mjs";

const { chromium } = playwright;
const { PNG } = pngjs;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(await readFile(path.join(repoRoot, "parity/manim-v0.21/manifest.json"), "utf8"));
const artifactRoot = path.resolve(repoRoot, process.env.NOON_MANIM_RASTER_ARTIFACTS ?? "manim-raster-artifacts");
const baseline = JSON.parse(await readFile(path.join(artifactRoot, "report.json"), "utf8"));
const reference = JSON.parse(await readFile(path.join(artifactRoot, "semantic/manim-all-frames.json"), "utf8"));
const backends = (process.env.NOON_MANIM_RASTER_BACKENDS ?? "webgpu,webgl").split(",").map((s) => s.trim());
const selectedIds = process.env.NOON_SHARED_PLAYBACK_FIXTURES?.split(",").map((s) => s.trim());
const fixtures = selectedIds ? manifest.fixtures.filter((f) => selectedIds.includes(f.id)) : manifest.fixtures;
if (selectedIds) assert.equal(fixtures.length, new Set(selectedIds).size, "unknown playback fixture");
assert.ok(backends.length > 0 && backends.every((b) => ["webgpu", "webgl"].includes(b)), "invalid backend");
assert.deepEqual(baseline.reference, manifest.reference, "dense raster reference configuration changed");
assert.equal(reference.manim_version, manifest.reference.version);
assert.equal(reference.frame_rate, manifest.reference.frame_rate);

// The corpus callbacks derive values from current shared state; they do not
// integrate arbitrary host dt. This qualifies that deterministic subset only.
// The existing raster pass is dense playback. This pass changes only the host
// sample cadence and reuses its actual pixels/current-runtime diagnostics.
const port = Number(process.env.NOON_SHARED_PLAYBACK_PORT ?? "4194");
const baseUrl = `http://127.0.0.1:${port}`;
const contentTypes = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript",
  ".wasm": "application/wasm", ".json": "application/json", ".py": "text/x-python" };
const server = createServer(async (request, response) => {
  try {
    const relative = decodeURIComponent(new URL(request.url, baseUrl).pathname).replace(/^\/+/, "");
    const resolved = path.resolve(repoRoot, relative);
    if (!resolved.startsWith(`${repoRoot}${path.sep}`)) { response.writeHead(403).end(); return; }
    if (!(await stat(resolved)).isFile()) { response.writeHead(404).end(); return; }
    response.setHeader("Content-Type", contentTypes[path.extname(resolved)] ?? "application/octet-stream");
    createReadStream(resolved).on("error", () => response.destroy()).pipe(response);
  } catch (error) {
    response.writeHead(error.code === "ENOENT" ? 404 : 500).end(String(error));
  }
});

function effectiveFrame(frame) {
  assert.equal(frame?.engine, "noon", "missing shared-runtime capture");
  // Publication epochs count work performed, so dense and sparse stepping can
  // differ. Object identities, painter order, presence and effective values must agree.
  const { publication, ...effective } = frame;
  assert.ok(publication, "missing capture publication provenance");
  return effective;
}

async function qualifyFixture(page, fixture, backend) {
  const denseFixture = baseline.fixtures.find((f) => f.id === fixture.id);
  assert.equal(denseFixture?.scene, fixture.scene, "dense fixture source selection changed");
  assert.equal(denseFixture.expectedDuration, fixture.expected_duration);
  const dense = denseFixture.backends[backend];
  assert.ok(dense && !dense.error && dense.samples.length > 0, "missing successful dense capture");
  const manim = reference.fixtures.find((f) => f.id === fixture.id);
  assert.equal(manim?.frame_count, manim?.frames.length, "invalid reference frame map");
  const times = dense.samples.map((sample) => {
    assert.equal(sample.time, manim.frames[sample.frameIndex]?.time, "dense sample is not a pinned frame");
    return sample.time;
  });
  times.push(fixture.expected_duration);
  const source = await readFile(path.join(repoRoot, fixture.source ?? manifest.reference.source), "utf8");
  const selected = rasterFixtureSource(source, fixture.scene);
  await page.goto(`${baseUrl}/web/manim-raster-host.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonHostRaster.ready());
  const loaded = await page.evaluate(({ source, duration }) => window.noonHostRaster.load(source, duration),
    { source: selected, duration: Math.max(1, fixture.expected_duration + 1) });
  assert.equal(loaded.kind, "semantic_execution");
  assert.equal(loaded.rendererBackend, backend === "webgpu" ? "WebGPU" : "WebGL2");
  const output = path.join(artifactRoot, "shared-playback", backend, fixture.id);
  await mkdir(output, { recursive: true });
  const samples = [];
  for (const [index, sample] of dense.samples.entries()) {
    const label = `frame-${String(sample.frameIndex).padStart(4, "0")}`;
    const metrics = await page.evaluate(({ index, times }) => window.noonHostRaster.renderThrough(index, times),
      { index, times });
    assert.equal(metrics.presented, true);
    assert.equal(metrics.time, sample.time);
    const sparsePath = path.join(output, `${label}.png`);
    await page.locator("#scene").screenshot({ path: sparsePath });
    const sparse = await page.evaluate(() => window.noonHostRaster.debugFrame());
    await writeFile(path.join(output, `${label}.json`), `${JSON.stringify(sparse, null, 2)}\n`);
    const maximumAbsoluteError = compareEffectiveFrames(effectiveFrame(sparse), effectiveFrame(sample.debugFrame));
    const expected = PNG.sync.read(await readFile(path.join(artifactRoot, backend, fixture.id, `${label}.png`)));
    const actual = PNG.sync.read(await readFile(sparsePath));
    assert.equal(actual.width, expected.width);
    assert.equal(actual.height, expected.height);
    assert.ok(actual.data.equals(expected.data), `${fixture.id}/${label}: pixels depend on sample cadence`);
    samples.push({ frameIndex: sample.frameIndex, time: sample.time, effectiveStateEqual: maximumAbsoluteError === 0, maximumAbsoluteError, rasterPixelsEqual: true });
  }
  const completed = await page.evaluate((times) => window.noonHostRaster.renderThrough(times.length - 1, times), times);
  assert.equal(completed.authoredDuration, fixture.expected_duration);
  return { id: fixture.id, scene: fixture.scene, samples };
}

const results = [];
const failures = [];
try {
  // Listen failure (including a port collision) rejects before any browser opens.
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", resolve); });
  for (const backend of backends) {
    const browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs(backend) });
    try {
      const context = await browser.newContext({ viewport: {
        width: manifest.reference.pixel_width + 40, height: manifest.reference.pixel_height + 40,
      } });
      const result = { backend, fixtures: [] };
      results.push(result);
      for (const fixture of fixtures) {
        const page = await context.newPage();
        let deadline;
        try {
          const capture = await Promise.race([qualifyFixture(page, fixture, backend), new Promise((_, reject) => {
            deadline = setTimeout(() => reject(new Error("shared playback fixture exceeded 60 seconds")), 60_000);
          })]);
          result.fixtures.push(capture);
          console.log(`[PASS] ${fixture.id}/${backend}: sparse == dense shared playback`);
        } catch (error) {
          const detail = error.stack ?? String(error);
          result.fixtures.push({ id: fixture.id, error: detail });
          failures.push(`${fixture.id}/${backend}: ${detail}`);
          console.error(`[FAIL] ${fixture.id}/${backend}: ${detail}`);
        } finally {
          clearTimeout(deadline);
          await page.close();
        }
      }
    } finally { await browser.close(); }
  }
  const reportPath = path.join(artifactRoot, "shared-playback-report.json");
  await writeFile(reportPath, `${JSON.stringify({ schemaVersion: 1, mode: "shared-sparse-vs-dense",
    maximumEffectiveAbsoluteError: MAX_EFFECTIVE_ABSOLUTE_ERROR,
    sourceRasterReport: "report.json", sourceSemanticReference: "semantic/manim-all-frames.json", results }, null, 2)}\n`);
  console.log(`Shared playback cadence report: ${reportPath}`);
  assert.equal(failures.length, 0, failures.join("\n"));
} finally {
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
}
