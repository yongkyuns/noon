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
const fixtureSources = new Map();
for (const fixture of manifest.fixtures) {
  const relativeSource = fixture.source ?? reference.source;
  if (!fixtureSources.has(relativeSource)) {
    fixtureSources.set(relativeSource, await readFile(path.join(repoRoot, relativeSource), "utf8"));
  }
}

function fixtureSourceFor(fixture) {
  const relativeSource = fixture.source ?? reference.source;
  const source = fixtureSources.get(relativeSource);
  assert.ok(source, `${fixture.id}: missing canonical source ${relativeSource}`);
  return source;
}

function fixtureSourcePathFor(fixture) {
  return path.join(repoRoot, fixture.source ?? reference.source);
}
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
const backends = (process.env.NOON_MANIM_RASTER_BACKENDS ?? "webgpu,webgl")
  .split(",")
  .map((value) => value.trim())
  .filter(Boolean);
const port = Number(process.env.NOON_MANIM_SEEK_PLAYBACK_PORT ?? "4192");
const baseUrl = `http://127.0.0.1:${port}`;
const MAX_PRESENT_ATTEMPTS = 8;

assert.equal(reference.version, "0.21.0", "seek/playback oracle must stay pinned to ManimCE 0.21.0");
assert.equal(
  rasterReport.reference.version,
  reference.version,
  "seek/playback oracle must consume the current pinned Manim raster report",
);
assert.equal(
  rasterReport.reference.frame_rate,
  reference.frame_rate,
  "seek/playback oracle frame rate must match the pinned Manim report",
);
assert.equal(
  semanticReference.manim_version,
  reference.version,
  "seek/playback oracle must consume the current semantic Manim reference",
);
assert.equal(
  semanticReference.frame_rate,
  reference.frame_rate,
  "semantic Manim frame rate must match the pinned raster report",
);
for (const backend of backends) {
  assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend ${backend}`);
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

async function authorNoonDocuments() {
  const browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: ["--disable-dev-shm-usage"],
  });
  try {
    const page = await browser.newPage();
    await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
    await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
    await page.evaluate(() => window.noonManimCompat.ready());
    const documents = new Map();
    for (const fixture of manifest.fixtures) {
      const result = await page.evaluate(
        (source) => window.noonManimCompat.run(source),
        noonSourceFor(fixture),
      );
      assert.equal(result.kind, "scene_document", `${fixture.id}: Noon authoring result kind`);
      assert.equal(result.duration, fixture.expected_duration, `${fixture.id}: authored Noon duration`);
      documents.set(fixture.id, result.document);
    }
    return documents;
  } finally {
    await browser.close();
  }
}

function frameTimesThrough(frameTimes, frameIndex) {
  assert.ok(
    Number.isSafeInteger(frameIndex) && frameIndex >= 0 && frameIndex < frameTimes.length,
    "invalid Manim frame index",
  );
  return frameTimes.slice(0, frameIndex + 1);
}

function logicalFrameTimes(fixtureId) {
  const semanticFixture = semanticByFixture.get(fixtureId);
  assert.ok(semanticFixture, `${fixtureId}: missing semantic Manim frame reference`);
  assert.equal(
    semanticFixture.frame_count,
    semanticFixture.frames.length,
    `${fixtureId}: semantic Manim frame count`,
  );
  const frameTimes = semanticFixture.frames.map((frame) => Number(frame.time));
  frameTimes.reduce((last, time, index) => {
    assert.ok(Number.isFinite(time) && time >= 0, `${fixtureId}/${index}: invalid Manim logical time`);
    assert.ok(
      time + 1e-12 >= last,
      `${fixtureId}/${index}: Manim logical frame times move backwards`,
    );
    return time;
  }, -Infinity);
  return frameTimes;
}

async function nextAnimationFrame(page) {
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)));
}

async function retryPresented(page, invoke, label) {
  let metrics = null;
  for (let attempt = 1; attempt <= MAX_PRESENT_ATTEMPTS; attempt += 1) {
    metrics = await invoke();
    assert.equal(metrics.error, null, `${label}: render error`);
    if (metrics.presented) {
      return { metrics, attempts: attempt };
    }
    await nextAnimationFrame(page);
  }
  assert.fail(
    `${label}: browser did not present a frame after ${MAX_PRESENT_ATTEMPTS} attempts; ` +
      `last metrics=${JSON.stringify(metrics)}`,
  );
}

async function presentDirect(page, time, label) {
  return retryPresented(
    page,
    () => page.evaluate((target) => window.noonSmoke.renderAt(target), time),
    label,
  );
}

async function beginIncremental(page, label) {
  return retryPresented(
    page,
    () => page.evaluate(() => window.noonSmoke.beginIncremental()),
    label,
  );
}

async function presentIncremental(page, time, label) {
  return retryPresented(
    page,
    () => page.evaluate((target) => window.noonSmoke.renderIncrementalAt(target), time),
    label,
  );
}

async function assertPngPixelsEqual(directPath, incrementalPath, label) {
  const direct = PNG.sync.read(await readFile(directPath));
  const incremental = PNG.sync.read(await readFile(incrementalPath));
  assert.equal(incremental.width, direct.width, `${label}: raster width changed`);
  assert.equal(incremental.height, direct.height, `${label}: raster height changed`);
  assert.ok(
    direct.data.equals(incremental.data),
    `${label}: direct seek and incremental playback produced different pixels`,
  );
}

async function createRenderPage(browser, expectedBackend, installSnapshotHelpers) {
  const page = await browser.newPage({
    viewport: { width: reference.pixel_width + 40, height: reference.pixel_height + 40 },
  });
  await page.goto(`${baseUrl}/web/browser-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonSmoke?.state.ready === true, null, {
    timeout: 30_000,
  });
  const initial = await page.evaluate(() => window.noonSmoke.metrics());
  assert.equal(initial.rendererBackend, expectedBackend, "selected renderer backend");
  if (installSnapshotHelpers) {
    await page.evaluate(async () => {
      const wasm = await import("./pkg/noon_web.js");
      await wasm.default();
      window.noonSeekPlayback = {
        direct: wasm.evaluateSceneSnapshot,
        playback: wasm.evaluateScenePlaybackSnapshot,
      };
    });
  }
  return page;
}

async function verifyBackend(backend, documents) {
  const browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs(backend),
  });
  const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
  try {
    const backendResult = { backend, fixtures: [] };
    for (const fixture of manifest.fixtures) {
      let directPage = null;
      let incrementalPage = null;
      try {
        // Keep the two evaluation modes on independent fresh canvas players. In
        // particular, WebGL compositors are allowed to retain the last presented
        // backbuffer while a new surface presentation becomes visible. Reusing one
        // player after all late direct samples would therefore make the incremental
        // frame-zero screenshot depend on the preceding direct-render history rather
        // than on the evaluation mode being tested.
        directPage = await createRenderPage(browser, expectedBackend, true);
        incrementalPage = await createRenderPage(browser, expectedBackend, false);

        const document = documents.get(fixture.id);
        const sceneJson = JSON.stringify(document);
        const reportFixture = rasterReport.fixtures.find((entry) => entry.id === fixture.id);
        assert.ok(reportFixture, `${fixture.id}: missing pinned Manim raster report fixture`);
        const reportBackend = reportFixture.backends[backend];
        assert.ok(reportBackend, `${fixture.id}: missing ${backend} raster report entry`);
        const samples = reportBackend.samples;
        assert.ok(samples.length > 0, `${fixture.id}: pinned Manim report has no samples`);
        const frameTimes = logicalFrameTimes(fixture.id);

        const [loadedDirect, loadedIncremental] = await Promise.all([
          directPage.evaluate((json) => window.noonSmoke.loadScene(json), sceneJson),
          incrementalPage.evaluate((json) => window.noonSmoke.loadScene(json), sceneJson),
        ]);
        assert.equal(
          loadedDirect.objectCount,
          document.objects.length,
          `${fixture.id}: direct player loaded object count`,
        );
        assert.equal(
          loadedIncremental.objectCount,
          document.objects.length,
          `${fixture.id}: incremental player loaded object count`,
        );

        const fixtureDir = path.join(artifactRoot, "seek-playback", backend, fixture.id);
        await mkdir(fixtureDir, { recursive: true });
        const directPaths = new Map();
        const directAttempts = new Map();
        const sampleResults = [];

        for (const sample of samples) {
          assert.ok(
            Number.isSafeInteger(sample.frameIndex) &&
              sample.frameIndex >= 0 &&
              sample.frameIndex < frameTimes.length,
            `${fixture.id}: raster report has invalid Manim frame index ${sample.frameIndex}`,
          );
          const expectedTime = frameTimes[sample.frameIndex];
          assert.ok(
            Math.abs(sample.time - expectedTime) <= 1e-12,
            `${fixture.id}/${sample.frameIndex}: raster report timestamp is not the semantic Manim frame time`,
          );
          const state = await directPage.evaluate(
            ({ json, time, times }) => ({
              direct: JSON.parse(window.noonSeekPlayback.direct(json, time)),
              incremental: JSON.parse(
                window.noonSeekPlayback.playback(json, JSON.stringify(times)),
              ),
            }),
            {
              json: sceneJson,
              time: sample.time,
              times: frameTimesThrough(frameTimes, sample.frameIndex),
            },
          );
          assert.deepEqual(
            state.incremental,
            state.direct,
            `${fixture.id}/${backend}/${sample.frameIndex}: normalized state differs between direct seek and Manim-frame incremental playback`,
          );

          const direct = await presentDirect(
            directPage,
            sample.time,
            `${fixture.id}/${backend}/frame-${sample.frameIndex}/direct`,
          );
          assert.ok(
            Math.abs(direct.metrics.time - sample.time) <= 1e-12,
            `${fixture.id}: direct render playhead drifted at ${sample.time}`,
          );
          await nextAnimationFrame(directPage);
          const directPath = path.join(fixtureDir, `${sample.frameIndex}-direct.png`);
          await directPage.locator("#scene").screenshot({ path: directPath });
          directPaths.set(sample.frameIndex, directPath);
          directAttempts.set(sample.frameIndex, direct.attempts);
        }

        const started = await beginIncremental(
          incrementalPage,
          `${fixture.id}/${backend}/incremental-start`,
        );
        assert.ok(Math.abs(started.metrics.time) <= 1e-12, `${fixture.id}: incremental start drifted`);
        let previousTime = 0.0;
        for (const sample of samples) {
          assert.ok(sample.time >= previousTime, `${fixture.id}: Manim samples are not monotonic`);
          const incremental = await presentIncremental(
            incrementalPage,
            sample.time,
            `${fixture.id}/${backend}/frame-${sample.frameIndex}/incremental`,
          );
          assert.ok(
            Math.abs(incremental.metrics.time - sample.time) <= 1e-12,
            `${fixture.id}: incremental render playhead drifted at ${sample.time}`,
          );
          await nextAnimationFrame(incrementalPage);
          const incrementalPath = path.join(fixtureDir, `${sample.frameIndex}-incremental.png`);
          await incrementalPage.locator("#scene").screenshot({ path: incrementalPath });
          const directPath = directPaths.get(sample.frameIndex);
          await assertPngPixelsEqual(
            directPath,
            incrementalPath,
            `${fixture.id}/${backend}/frame-${sample.frameIndex}`,
          );
          sampleResults.push({
            frameIndex: sample.frameIndex,
            time: sample.time,
            normalizedStateEqual: true,
            rasterPixelsEqual: true,
            directPresentationAttempts: directAttempts.get(sample.frameIndex),
            incrementalPresentationAttempts: incremental.attempts,
          });
          previousTime = sample.time;
        }

        backendResult.fixtures.push({
          id: fixture.id,
          scene: fixture.scene,
          independentCanvasPlayers: true,
          incrementalStartPresentationAttempts: started.attempts,
          samples: sampleResults,
        });
        console.log(
          `✓ ${fixture.id} ${backend}: direct seek == incremental playback at ${sampleResults.length} pinned Manim frames`,
        );
      } finally {
        await directPage?.close();
        await incrementalPage?.close();
      }
    }
    return backendResult;
  } finally {
    await browser.close();
  }
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
      const response = await fetch(`${baseUrl}/web/browser-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`seek/playback raster server did not start: ${lastError}\n${serverOutput}`);
}

try {
  await waitForServer();
  const documents = await authorNoonDocuments();
  const results = [];
  for (const backend of backends) {
    results.push(await verifyBackend(backend, documents));
  }
  const reportPath = path.join(artifactRoot, "seek-playback-report.json");
  await writeFile(
    reportPath,
    `${JSON.stringify(
      {
        manimVersion: reference.version,
        frameRate: reference.frame_rate,
        sourceRasterReport: "report.json",
        sourceSemanticReference: "semantic/manim-all-frames.json",
        maxPresentationAttempts: MAX_PRESENT_ATTEMPTS,
        independentCanvasPlayers: true,
        results,
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Pinned Manim seek/playback differential report: ${reportPath}`);
} finally {
  server.kill("SIGTERM");
}
