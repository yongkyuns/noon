import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

const { chromium } = playwright;
const { PNG } = pngjs;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const manifest = JSON.parse(
  await readFile(path.join(repoRoot, "parity", "manim-v0.21", "manifest.json"), "utf8"),
);
const reference = manifest.reference;
const artifactRoot = path.resolve(
  repoRoot,
  process.env.NOON_MANIM_RASTER_ARTIFACTS ?? "manim-raster-artifacts",
);
const rasterReport = JSON.parse(await readFile(path.join(artifactRoot, "report.json"), "utf8"));
const semanticReference = JSON.parse(
  await readFile(path.join(artifactRoot, "semantic", "manim-all-frames.json"), "utf8"),
);
const semanticByFixture = new Map(
  semanticReference.fixtures.map((fixture) => [fixture.id, fixture]),
);
const fixtureSources = new Map();
for (const fixture of manifest.fixtures) {
  const relativeSource = fixture.source ?? reference.source;
  if (!fixtureSources.has(relativeSource)) {
    fixtureSources.set(relativeSource, await readFile(path.join(repoRoot, relativeSource), "utf8"));
  }
}
const backends = (process.env.NOON_MANIM_RASTER_BACKENDS ?? "webgpu,webgl")
  .split(",")
  .map((value) => value.trim())
  .filter(Boolean);
const port = Number(process.env.NOON_MANIM_RETAINED_SEEK_PORT ?? "4195");
const baseUrl = `http://127.0.0.1:${port}`;

function fixtureSourceFor(fixture) {
  const source = fixtureSources.get(fixture.source ?? reference.source);
  assert.ok(source, `${fixture.id}: missing canonical source`);
  return source;
}

function noonSourceFor(fixture) {
  const adapted = fixtureSourceFor(fixture).replace("from manim import *", "from noon import *");
  return `${adapted}\n\nresult = ${fixture.scene}()\nresult.setup()\ntry:\n    result.construct()\nfinally:\n    result.tear_down()\n`;
}

function browserArgs(backend) {
  if (backend === "webgpu") {
    return [
      "--enable-unsafe-webgpu",
      "--enable-unsafe-swiftshader",
      "--use-webgpu-adapter=swiftshader",
      "--use-gpu-in-tests",
      "--ignore-gpu-blocklist",
      "--enable-features=Vulkan",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--use-vulkan=swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ];
  }
  return [
    "--disable-features=WebGPU",
    "--enable-unsafe-swiftshader",
    "--ignore-gpu-blocklist",
    "--use-gl=angle",
    "--use-angle=swiftshader",
    "--disable-gpu-sandbox",
    "--disable-dev-shm-usage",
  ];
}

function frameTimes(fixtureId) {
  const fixture = semanticByFixture.get(fixtureId);
  assert.ok(fixture, `${fixtureId}: missing Manim semantic reference`);
  assert.equal(fixture.frame_count, fixture.frames.length, `${fixtureId}: semantic frame count`);
  return fixture.frames.map((frame, index) => {
    const time = Number(frame.time);
    assert.ok(Number.isFinite(time) && time >= 0, `${fixtureId}/${index}: invalid frame time`);
    return time;
  });
}

async function retainedFixtures() {
  const browser = await chromium.launch({ channel: "chromium", headless: true });
  try {
    const page = await browser.newPage();
    await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
    await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
    await page.evaluate(() => window.noonManimCompat.ready());
    const retained = [];
    for (const fixture of manifest.fixtures) {
      const result = await page.evaluate(
        (source) => window.noonManimCompat.run(source),
        noonSourceFor(fixture),
      );
      if ((result.retainedDocument?.objects?.length ?? 0) > 0) retained.push(fixture);
    }
    return retained;
  } finally {
    await browser.close();
  }
}

async function createHostPage(browser, fixture, expectedBackend) {
  const page = await browser.newPage({
    viewport: { width: reference.pixel_width + 40, height: reference.pixel_height + 40 },
  });
  await page.goto(`${baseUrl}/web/manim-raster-host.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonHostRaster.ready());
  const loaded = await page.evaluate(
    ({ source, duration }) => window.noonHostRaster.load(source, Math.max(1, duration + 1)),
    { source: noonSourceFor(fixture), duration: fixture.expected_duration },
  );
  assert.equal(loaded.retained, true, `${fixture.id}: retained seek fixture must use retained execution`);
  assert.equal(loaded.duration, fixture.expected_duration, `${fixture.id}: authored duration`);
  assert.equal(loaded.rendererBackend, expectedBackend, `${fixture.id}: renderer backend`);
  return page;
}

async function assertPngEqual(leftPath, rightPath, label) {
  const left = PNG.sync.read(await readFile(leftPath));
  const right = PNG.sync.read(await readFile(rightPath));
  assert.equal(right.width, left.width, `${label}: width`);
  assert.equal(right.height, left.height, `${label}: height`);
  assert.ok(left.data.equals(right.data), `${label}: direct seek and incremental retained pixels differ`);
}

async function verifyBackend(backend, fixtures) {
  const browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs(backend) });
  const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
  const result = { backend, fixtures: [] };
  try {
    for (const fixture of fixtures) {
      const reportFixture = rasterReport.fixtures.find((entry) => entry.id === fixture.id);
      assert.ok(reportFixture, `${fixture.id}: missing raster report fixture`);
      const samples = reportFixture.backends[backend]?.samples;
      assert.ok(samples?.length > 0, `${fixture.id}: missing ${backend} raster samples`);
      const times = frameTimes(fixture.id);
      const fixtureDir = path.join(artifactRoot, "retained-seek-playback", backend, fixture.id);
      await mkdir(fixtureDir, { recursive: true });
      const incrementalPage = await createHostPage(browser, fixture, expectedBackend);
      const sampleResults = [];
      try {
        for (const sample of samples) {
          assert.ok(
            Number.isSafeInteger(sample.frameIndex) && sample.frameIndex >= 0 && sample.frameIndex < times.length,
            `${fixture.id}: invalid sample frame ${sample.frameIndex}`,
          );
          assert.ok(
            Math.abs(Number(sample.time) - Number(times[sample.frameIndex])) <= 1e-12,
            `${fixture.id}/${sample.frameIndex}: sample time is not the pinned Manim frame time`,
          );

          const directPage = await createHostPage(browser, fixture, expectedBackend);
          const directPath = path.join(fixtureDir, `${sample.frameIndex}-direct.png`);
          try {
            const direct = await directPage.evaluate(
              (time) => window.noonHostRaster.renderAt(time),
              sample.time,
            );
            assert.equal(direct.error, null, `${fixture.id}: direct retained render error`);
            assert.equal(direct.presented, true, `${fixture.id}: direct retained frame not presented`);
            assert.ok(
              Math.abs(Number(direct.time) - Number(sample.time)) <= 1e-12,
              `${fixture.id}: direct retained logical time drift`,
            );
            assert.ok(
              Math.abs(Number(direct.rendererTime) - Number(sample.time)) <= 1e-12,
              `${fixture.id}: direct retained renderer time drift`,
            );
            await directPage.locator("#scene").screenshot({ path: directPath });
          } finally {
            await directPage.close();
          }

          const incremental = await incrementalPage.evaluate(
            ({ frameIndex, times }) => window.noonHostRaster.renderThrough(frameIndex, times),
            { frameIndex: sample.frameIndex, times },
          );
          assert.equal(incremental.error, null, `${fixture.id}: incremental retained render error`);
          assert.equal(incremental.presented, true, `${fixture.id}: incremental retained frame not presented`);
          assert.ok(
            Math.abs(Number(incremental.time) - Number(sample.time)) <= 1e-12,
            `${fixture.id}: incremental retained logical time drift`,
          );
          assert.ok(
            Math.abs(Number(incremental.rendererTime) - Number(sample.time)) <= 1e-12,
            `${fixture.id}: incremental retained renderer time drift`,
          );
          const incrementalPath = path.join(fixtureDir, `${sample.frameIndex}-incremental.png`);
          await incrementalPage.locator("#scene").screenshot({ path: incrementalPath });
          await assertPngEqual(
            directPath,
            incrementalPath,
            `${fixture.id}/${backend}/frame-${sample.frameIndex}`,
          );
          sampleResults.push({
            frameIndex: sample.frameIndex,
            time: sample.time,
            rasterPixelsEqual: true,
            independentDirectCanvas: true,
          });
        }
      } finally {
        await incrementalPage.close();
      }
      result.fixtures.push({ id: fixture.id, scene: fixture.scene, samples: sampleResults });
      console.log(
        `✓ ${fixture.id} ${backend}: retained direct seek == incremental playback at ${sampleResults.length} pinned Manim frames`,
      );
    }
  } finally {
    await browser.close();
  }
  return result;
}

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/manim-raster-host.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`retained seek/playback server did not start: ${lastError}\n${serverOutput}`);
}

try {
  await waitForServer();
  const fixtures = await retainedFixtures();
  assert.ok(fixtures.length > 0, "canonical Manim manifest must contain retained execution coverage");
  const results = [];
  for (const backend of backends) results.push(await verifyBackend(backend, fixtures));
  const reportPath = path.join(artifactRoot, "retained-seek-playback-report.json");
  await writeFile(
    reportPath,
    `${JSON.stringify(
      {
        manimVersion: reference.version,
        frameRate: reference.frame_rate,
        sourceRasterReport: "report.json",
        sourceSemanticReference: "semantic/manim-all-frames.json",
        retainedFixtureIds: fixtures.map((fixture) => fixture.id),
        results,
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Pinned Manim retained seek/playback report: ${reportPath}`);
} finally {
  server.kill("SIGTERM");
}
