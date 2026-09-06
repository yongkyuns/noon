import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtemp, mkdir, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

const { chromium } = playwright;
const { PNG } = pngjs;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const sceneDir = await mkdtemp(path.join(tmpdir(), "noon-browser-scenes-"));
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_BROWSER_SMOKE_ARTIFACTS ?? "browser-smoke-artifacts",
);
const port = Number(process.env.NOON_BROWSER_SMOKE_PORT ?? "4173");
const baseUrl = `http://127.0.0.1:${port}`;
const backendMode = process.env.NOON_BROWSER_SMOKE_BACKEND ?? "webgpu";
assert.ok(
  backendMode === "webgpu" || backendMode === "webgl",
  `unknown browser smoke backend: ${backendMode}`,
);
const expectedRendererBackend = backendMode === "webgpu" ? "WebGPU" : "WebGL2";

await mkdir(artifactDir, { recursive: true });

const generated = spawnSync(
  "python3",
  ["web/python/playground_examples.py", sceneDir],
  {
    cwd: repoRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      PYTHONDONTWRITEBYTECODE: "1",
    },
  },
);
if (generated.status !== 0) {
  throw new Error(
    `Unable to generate playground scenes:\n${generated.stdout}\n${generated.stderr}`,
  );
}

const examples = generated.stdout
  .trim()
  .split("\n")
  .filter(Boolean)
  .map((line) => {
    const separator = line.indexOf("\t");
    if (separator === -1) {
      throw new Error(`Unexpected playground generator output: ${line}`);
    }
    return {
      name: line.slice(0, separator),
      file: line.slice(separator + 1),
    };
  });

const gallerySource = await readFile(path.join(repoRoot, "web/main.js"), "utf8");
assert.ok(
  gallerySource.includes("loadGalleryManifest"),
  "public example gallery must load its catalog from the Manim manifest",
);
assert.ok(
  !gallerySource.includes("const SCENE_EXAMPLES = ["),
  "public example gallery must not restore a hard-coded scene catalog",
);
const manimManifest = JSON.parse(
  await readFile(
    path.join(repoRoot, "web/python/examples/manim_tutorial_manifest.json"),
    "utf8",
  ),
);
const browserAuthoredManim = manimManifest.entries.filter(
  (entry) => entry.status === "ready",
);
assert.ok(browserAuthoredManim.length > 0, "public gallery must expose ready Manim examples");
for (const entry of browserAuthoredManim) {
  assert.equal(
    entry.reuse,
    "source-equivalent-manim-v0.21",
    `${entry.id}: public example must remain source-equivalent ManimCE`,
  );
}
const browserAuthoredManimCount = browserAuthoredManim.length;

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  {
    cwd: repoRoot,
    stdio: ["ignore", "pipe", "pipe"],
  },
);
server.stdout.on("data", (chunk) => {
  serverOutput += chunk;
});
server.stderr.on("data", (chunk) => {
  serverOutput += chunk;
});

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/browser-smoke.html`);
      if (response.ok) {
        return;
      }
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Browser smoke server did not start: ${lastError}\n${serverOutput}`);
}

function artifactName(index, name, checkpoint) {
  const slug = name
    .normalize("NFKD")
    .replace(/[^a-zA-Z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    .toLowerCase();
  return `${String(index).padStart(2, "0")}-${slug || "scene"}-${checkpoint}.png`;
}

function visiblePixelStats(buffer, name) {
  const png = PNG.sync.read(buffer);
  assert.ok(png.width >= 320 && png.height >= 180, `${name}: canvas screenshot is too small`);

  const background = [png.data[0], png.data[1], png.data[2], png.data[3]];
  let changedPixels = 0;
  let minX = png.width;
  let minY = png.height;
  let maxX = -1;
  let maxY = -1;
  for (let offset = 0; offset < png.data.length; offset += 4) {
    const distance =
      Math.abs(png.data[offset] - background[0]) +
      Math.abs(png.data[offset + 1] - background[1]) +
      Math.abs(png.data[offset + 2] - background[2]) +
      Math.abs(png.data[offset + 3] - background[3]);
    if (distance >= 32) {
      changedPixels += 1;
      const pixel = offset / 4;
      const x = pixel % png.width;
      const y = Math.floor(pixel / png.width);
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x);
      maxY = Math.max(maxY, y);
    }
  }
  return { changedPixels, bounds: { minX, minY, maxX, maxY } };
}

function differingPixelCount(beforeBuffer, afterBuffer, region) {
  const before = PNG.sync.read(beforeBuffer);
  const after = PNG.sync.read(afterBuffer);
  assert.equal(before.width, after.width, "pixel diff width mismatch");
  assert.equal(before.height, after.height, "pixel diff height mismatch");
  let differing = 0;
  const minX = Math.max(0, Math.floor(region.minX * before.width));
  const maxX = Math.min(before.width - 1, Math.ceil(region.maxX * before.width));
  const minY = Math.max(0, Math.floor(region.minY * before.height));
  const maxY = Math.min(before.height - 1, Math.ceil(region.maxY * before.height));
  for (let y = minY; y <= maxY; y += 1) {
    for (let x = minX; x <= maxX; x += 1) {
      const offset = (y * before.width + x) * 4;
      const distance =
        Math.abs(before.data[offset] - after.data[offset]) +
        Math.abs(before.data[offset + 1] - after.data[offset + 1]) +
        Math.abs(before.data[offset + 2] - after.data[offset + 2]) +
        Math.abs(before.data[offset + 3] - after.data[offset + 3]);
      if (distance >= 32) {
        differing += 1;
      }
    }
  }
  return differing;
}

function pixelAt(buffer, xFraction, yFraction) {
  const png = PNG.sync.read(buffer);
  const x = Math.max(0, Math.min(png.width - 1, Math.round((png.width - 1) * xFraction)));
  const y = Math.max(0, Math.min(png.height - 1, Math.round((png.height - 1) * yFraction)));
  const offset = (y * png.width + x) * 4;
  return [
    png.data[offset],
    png.data[offset + 1],
    png.data[offset + 2],
    png.data[offset + 3],
  ];
}

function pixelDistance(left, right) {
  return left.reduce((distance, channel, index) => distance + Math.abs(channel - right[index]), 0);
}

function latestSceneEnd(document) {
  assert.ok(document.tracks.length > 0, "playground scene must contain at least one track");
  return Math.max(
    ...document.tracks.map(
      (track) => track.timing.start_time + track.timing.duration,
    ),
  );
}

function sampleTimes(latestEnd) {
  assert.ok(Number.isFinite(latestEnd) && latestEnd > 0, "scene timeline must have positive duration");
  assert.ok(latestEnd < 4.0, "scene timeline must fit the four-second playground loop");
  return [0.35, 0.60, 0.85, 1.0].map((fraction) => latestEnd * fraction);
}

async function renderAndCapture(page, time, screenshotPath) {
  const metrics = await page.evaluate(
    (sceneTime) => window.noonSmoke.renderAt(sceneTime),
    time,
  );
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => resolve())));
  const screenshot = await page.locator("#scene").screenshot({ path: screenshotPath });
  return { metrics, screenshot };
}

async function directExecutionProof(page, expectedBackend) {
  await page.waitForFunction(() => window.noonDirectExecutionSmoke?.ready === true, null, {
    timeout: 30_000,
  });
  const direct = await page.evaluate(() => window.noonDirectExecutionSmoke);
  assert.equal(direct.error, null, `direct Rust/WASM execution proof failed: ${direct.error}`);
  assert.ok(direct.metrics, "direct Rust/WASM execution proof did not publish metrics");
  assert.equal(
    direct.metrics.backend,
    expectedBackend,
    `direct Rust/WASM execution selected ${direct.metrics.backend}; expected ${expectedBackend}`,
  );
  assert.equal(direct.metrics.presented, true, "direct Rust/WASM execution did not present");
  assert.equal(
    direct.metrics.objectCount,
    3,
    "direct Rust/WASM scene must retain animated geometry, camera, and text",
  );
  assert.ok(direct.metrics.drawCalls > 0, "direct Rust/WASM execution emitted no draw calls");
  assert.ok(direct.metrics.textDrawCalls > 0, "direct Rust/WASM execution emitted no text draw calls");
  assert.ok(
    direct.metrics.drawCalls > direct.metrics.textDrawCalls,
    "direct Rust/WASM execution emitted no geometry draw calls alongside text",
  );
  assert.ok(
    direct.metrics.authoredTime >= 0.1,
    "direct Rust/WASM execution did not settle its authored animation",
  );
  assert.ok(
    direct.metrics.scheduledAnimationFrames > 0,
    "direct Rust/WASM execution never scheduled its animation wake",
  );
  assert.equal(
    direct.metrics.staticFrameSkipped,
    true,
    "direct Rust/WASM renderer prepared a static frame without a publication",
  );
  assert.equal(
    direct.metrics.affineCallbacks?.backend,
    expectedBackend,
    "direct Rust/WASM callbacks did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.affineCallbacks?.authoredTime,
    2,
    "direct Rust/WASM callbacks did not reach the exact authored endpoint",
  );
  assert.equal(
    direct.metrics.affineCallbacks?.objectCount,
    3,
    "direct Rust/WASM callback scene did not retain its Text and two geometry objects",
  );
  assert.ok(
    direct.metrics.affineCallbacks?.sourceLuma >= 180,
    "direct Rust/WASM ordered callbacks did not render the transformed translucent source",
  );
  assert.ok(
    direct.metrics.affineCallbacks?.driftLuma >= 600,
    "direct Rust/WASM dt callback did not render the accumulated drift object",
  );
  assert.ok(
    direct.metrics.affineCallbacks?.driftLuma >=
      direct.metrics.affineCallbacks?.sourceLuma + 120,
    "direct Rust/WASM ordered opacity callback did not affect the source",
  );
  assert.ok(
    direct.metrics.affineCallbacks?.bootstrapVacatedLuma <= 60,
    "direct Rust/WASM time-zero callback did not publish before the first renderer frame",
  );
  assert.equal(
    direct.metrics.affineCompletion?.backend,
    expectedBackend,
    "direct Rust/WASM completion did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.affineCompletion?.authoredTime,
    4.25,
    "direct Rust/WASM completion did not preserve the typed authored sequence",
  );
  assert.equal(
    direct.metrics.affineCompletion?.objectCount,
    1,
    "direct Rust/WASM completion scene did not retain its Rust-authored object",
  );
  assert.ok(
    direct.metrics.affineCompletion?.endpointLuma >= 150,
    "direct Rust/WASM completion did not render the x=5 endpoint",
  );
  assert.ok(
    direct.metrics.affineCompletion?.priorSetterLuma <= 60,
    "direct Rust/WASM completion remained at the intervening x=3 setter",
  );
  assert.equal(
    direct.metrics.ordinaryAffinePlay?.backend,
    expectedBackend,
    "direct Rust/WASM ordinary affine play did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.ordinaryAffinePlay?.authoredTime,
    4,
    "direct Rust/WASM ordinary affine play did not preserve the shared session time",
  );
  assert.equal(
    direct.metrics.ordinaryAffinePlay?.objectCount,
    1,
    "direct Rust/WASM ordinary affine scene did not retain its Rust-authored object",
  );
  assert.ok(
    direct.metrics.ordinaryAffinePlay?.endpointLuma >= 250,
    "direct Rust/WASM ordinary affine play did not render the x=5 endpoint",
  );
  assert.ok(
    direct.metrics.ordinaryAffinePlay?.firstEndpointLuma <= 60 &&
      direct.metrics.ordinaryAffinePlay?.shiftedLuma <= 60,
    "direct Rust/WASM ordinary affine play retained an earlier barrier position",
  );
  assert.equal(
    direct.metrics.ordinaryAffineContinuation?.backend,
    expectedBackend,
    "direct Rust/WASM continuation did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.ordinaryAffineContinuation?.authoredTime,
    4,
    "direct Rust/WASM continuation did not retain its shared authored time",
  );
  assert.ok(
    direct.metrics.ordinaryAffineContinuation?.firstMidpointLuma >= 180 &&
      direct.metrics.ordinaryAffineContinuation?.secondMidpointLuma >= 180 &&
      direct.metrics.ordinaryAffineContinuation?.finalLuma >= 180,
    "direct Rust/WASM continuation did not visibly render both midpoints and final endpoint",
  );
  assert.ok(
    direct.metrics.ordinaryAffineContinuation?.noSyntheticWaitDraw &&
      direct.metrics.ordinaryAffineContinuation?.waitDelayMs >= 999 &&
      direct.metrics.ordinaryAffineContinuation?.waitDelayMs <= 1001 &&
      direct.metrics.ordinaryAffineContinuation?.finalCadence === "idle",
    "direct Rust/WASM continuation did not preserve its wait deadline and final resume lifecycle",
  );
  assert.equal(
    direct.metrics.ordinaryFadePlay?.backend,
    expectedBackend,
    "direct Rust/WASM ordinary fade did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.ordinaryFadePlay?.authoredTime,
    2.25,
    "direct Rust/WASM ordinary fade did not preserve its shared authored time",
  );
  assert.ok(
    direct.metrics.ordinaryFadePlay?.detachedLuma <= 30 &&
      direct.metrics.ordinaryFadePlay?.fadeInMidpointLuma >= 100 &&
      direct.metrics.ordinaryFadePlay?.fadeInEndpointLuma >= 300 &&
      direct.metrics.ordinaryFadePlay?.fadeOutMidpointLuma >= 100 &&
      direct.metrics.ordinaryFadePlay?.absentLuma <= 30,
    "direct Rust/WASM ordinary fade did not render both fade midpoints and its absent endpoint",
  );
  assert.ok(
    direct.metrics.ordinaryFadePlay?.absentObjectCount === 0 &&
      direct.metrics.ordinaryFadePlay?.readdedObjectCount === 1 &&
      direct.metrics.ordinaryFadePlay?.readdedColor.blue >= 200 &&
      direct.metrics.ordinaryFadePlay?.noSyntheticDetachedDraw &&
      direct.metrics.ordinaryFadePlay?.waitDelayMs >= 249 &&
      direct.metrics.ordinaryFadePlay?.waitDelayMs <= 251 &&
      direct.metrics.ordinaryFadePlay?.finalCadence === "idle",
    "direct Rust/WASM ordinary fade did not preserve detached wait and same-handle re-entry",
  );
  assert.equal(
    direct.metrics.ordinaryAffineCallbackContinuation?.backend,
    expectedBackend,
    "direct Rust/WASM callback continuation did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.ordinaryAffineCallbackContinuation?.authoredTime,
    1,
    "direct Rust/WASM callback continuation did not reach its exact endpoint",
  );
  assert.ok(
    direct.metrics.ordinaryAffineCallbackContinuation?.midpointColor.blue >= 70 &&
      direct.metrics.ordinaryAffineCallbackContinuation?.endpointColor.blue >= 70 &&
      direct.metrics.ordinaryAffineCallbackContinuation?.midpointVacatedLuma <= 60 &&
      direct.metrics.ordinaryAffineCallbackContinuation?.endpointVacatedLuma <= 60,
    "direct Rust/WASM callback continuation did not render its ordered midpoint and endpoint",
  );
  assert.equal(
    direct.metrics.ordinaryCallbackSparseReads?.backend,
    expectedBackend,
    "direct Rust/WASM sparse callback reads did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.ordinaryCallbackSparseReads?.authoredTime,
    1.5,
    "direct Rust/WASM sparse callback reads did not preserve the shared session time",
  );
  assert.ok(
    direct.metrics.ordinaryCallbackSparseReads?.initialRead.blue >= 180 &&
      direct.metrics.ordinaryCallbackSparseReads?.initialVacatedLuma <= 60 &&
      direct.metrics.ordinaryCallbackSparseReads?.midpoint.blue >= 180 &&
      direct.metrics.ordinaryCallbackSparseReads?.persistentHold.blue >= 180 &&
      direct.metrics.ordinaryCallbackSparseReads?.anchor.blue >= 180,
    "direct Rust/WASM sparse callback reads did not render initial, midpoint, Hold, and anchor states",
  );
  assert.equal(
    direct.metrics.ordinaryCompositionPlay?.backend,
    expectedBackend,
    "direct Rust/WASM ordinary composition did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.ordinaryCompositionPlay?.authoredTime,
    4,
    "direct Rust/WASM ordinary composition did not preserve its shared root time",
  );
  assert.equal(
    direct.metrics.ordinaryCompositionPlay?.objectCount,
    2,
    "direct Rust/WASM ordinary composition did not retain both Rust-authored objects",
  );
  assert.ok(
    direct.metrics.ordinaryCompositionPlay?.leftColor.green >= 180 &&
      direct.metrics.ordinaryCompositionPlay?.rightColor.blue >= 180 &&
      direct.metrics.ordinaryCompositionPlay?.oldLeftLuma <= 60 &&
      direct.metrics.ordinaryCompositionPlay?.oldRightLuma <= 60,
    "direct Rust/WASM ordinary composition did not render its released Parallel/Sequence endpoint",
  );
  assert.equal(
    direct.metrics.ordinaryCompositionContinuation?.backend,
    expectedBackend,
    "direct Rust/WASM composition continuation did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.ordinaryCompositionContinuation?.authoredTime,
    4,
    "direct Rust/WASM composition continuation did not retain its shared authored time",
  );
  assert.equal(
    direct.metrics.ordinaryCompositionContinuation?.objectCount,
    2,
    "direct Rust/WASM composition continuation did not retain both Rust-authored objects",
  );
  assert.ok(
    direct.metrics.ordinaryCompositionContinuation?.leftSequence.red >= 120 &&
      direct.metrics.ordinaryCompositionContinuation?.rightSequence.red >= 120 &&
      direct.metrics.ordinaryCompositionContinuation?.leftFinal.green >=
        direct.metrics.ordinaryCompositionContinuation?.leftFinal.red + 40 &&
      direct.metrics.ordinaryCompositionContinuation?.rightFinal.blue >=
        direct.metrics.ordinaryCompositionContinuation?.rightFinal.red + 40 &&
      direct.metrics.ordinaryCompositionContinuation?.finalCadence === "idle",
    "direct Rust/WASM composition continuation did not render its sequence and post-completion edit",
  );
  assert.equal(
    direct.metrics.ordinaryStylePlay?.backend,
    expectedBackend,
    "direct Rust/WASM ordinary style play did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.ordinaryStylePlay?.authoredTime,
    2,
    "direct Rust/WASM ordinary style play did not preserve the shared session time",
  );
  assert.equal(
    direct.metrics.ordinaryStylePlay?.objectCount,
    1,
    "direct Rust/WASM ordinary style scene did not retain its Rust-authored object",
  );
  assert.ok(
    direct.metrics.ordinaryStylePlay?.endpointColor.green >= 180 &&
      direct.metrics.ordinaryStylePlay?.endpointColor.green >=
        direct.metrics.ordinaryStylePlay?.endpointColor.red + 100 &&
      direct.metrics.ordinaryStylePlay?.endpointColor.green >=
        direct.metrics.ordinaryStylePlay?.endpointColor.blue + 100,
    "direct Rust/WASM ordinary style play did not render its post-completion green edit",
  );
  assert.equal(
    direct.metrics.ordinaryPaintPlay?.backend,
    expectedBackend,
    "direct Rust/WASM ordinary paint play did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.ordinaryPaintPlay?.authoredTime,
    2.4,
    "direct Rust/WASM ordinary paint play did not preserve the shared session time",
  );
  assert.equal(
    direct.metrics.ordinaryPaintPlay?.objectCount,
    1,
    "direct Rust/WASM ordinary paint scene did not retain its Rust-authored object",
  );
  assert.ok(
    direct.metrics.ordinaryPaintPlay?.endpointColor.red >= 180 &&
      direct.metrics.ordinaryPaintPlay?.endpointColor.green >= 180 &&
      direct.metrics.ordinaryPaintPlay?.endpointColor.blue <= 100,
    "direct Rust/WASM ordinary paint play did not render its post-completion yellow edit",
  );
  assert.equal(
    direct.metrics.ordinaryValueTrackerContinuation?.backend,
    expectedBackend,
    "direct Rust/WASM scalar continuation did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.ordinaryValueTrackerContinuation?.authoredTime,
    4,
    "direct Rust/WASM scalar continuation did not preserve its shared session time",
  );
  assert.equal(
    direct.metrics.ordinaryValueTrackerContinuation?.objectCount,
    1,
    "direct Rust/WASM scalar continuation did not retain its bound object",
  );
  for (const [label, color] of Object.entries({
    firstMidpoint: direct.metrics.ordinaryValueTrackerContinuation?.firstMidpoint,
    persistentHold: direct.metrics.ordinaryValueTrackerContinuation?.persistentHold,
    secondMidpoint: direct.metrics.ordinaryValueTrackerContinuation?.secondMidpoint,
    endpoint: direct.metrics.ordinaryValueTrackerContinuation?.endpoint,
  })) {
    assert.ok(
      color?.red >= 180 && color?.green >= 180 && color?.blue >= 180,
      `direct Rust/WASM scalar continuation did not render its ${label} state`,
    );
  }
  assert.equal(
    direct.metrics.nativeSignals?.backend,
    expectedBackend,
    "direct Rust/WASM native signals did not use the selected renderer backend",
  );
  assert.equal(
    direct.metrics.nativeSignals?.objectCount,
    1,
    "direct Rust/WASM native signals did not reveal the Rust-authored object",
  );
  assert.ok(
    direct.metrics.nativeSignals?.hiddenLuma <= 60 &&
      direct.metrics.nativeSignals?.visibleLuma >= 200,
    "direct Rust/WASM key state did not drive presence",
  );
  assert.ok(
    direct.metrics.nativeSignals?.vacatedLuma <= 60 &&
      direct.metrics.nativeSignals?.movedLuma >= 200,
    "direct Rust/WASM pointer state did not drive translation",
  );
  assert.ok(
    direct.metrics.nativeSignals?.dimmedLuma < direct.metrics.nativeSignals?.movedLuma * 0.7,
    "direct Rust/WASM scalar control did not drive opacity",
  );
  assert.ok(
    direct.metrics.nativeSignals?.firstClickLuma >= 60 &&
      direct.metrics.nativeSignals?.secondClickLuma <= 60,
    "direct Rust/WASM ordered pointer events did not drive rotation",
  );
  return direct.metrics;
}

let browser = null;
try {
  await waitForServer();
  const browserArgs =
    backendMode === "webgpu"
      ? [
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
        ]
      : [
          "--disable-features=WebGPU",
          "--enable-unsafe-swiftshader",
          "--ignore-gpu-blocklist",
          "--use-gl=angle",
          "--use-angle=swiftshader",
          "--disable-gpu-sandbox",
          "--disable-dev-shm-usage",
        ];
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs,
  });

  const page = await browser.newPage({ viewport: { width: 1000, height: 600 } });
  const browserErrors = [];
  const visualFailures = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") {
      browserErrors.push(`console: ${message.text()}`);
    }
  });

  await page.goto(`${baseUrl}/web/browser-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonSmoke?.state.ready === true, null, {
    timeout: 30_000,
  });

  const initial = await page.evaluate(() => window.noonSmoke.metrics());
  if (initial.error) {
    throw new Error(`${expectedRendererBackend} harness failed to initialize: ${initial.error}`);
  }
  assert.equal(
    initial.rendererBackend,
    expectedRendererBackend,
    `browser smoke selected ${initial.rendererBackend}; expected ${expectedRendererBackend}`,
  );
  const directMetrics = await directExecutionProof(page, expectedRendererBackend);

  for (const [index, example] of examples.entries()) {
    const sceneJson = await readFile(example.file, "utf8");
    const document = JSON.parse(sceneJson);
    const expectedObjects = document.objects.length;
    const latestEnd = latestSceneEnd(document);
    assert.ok(expectedObjects > 0, `${example.name}: scene has no semantic objects`);

    const loaded = await page.evaluate(
      (json) => window.noonSmoke.loadScene(json),
      sceneJson,
    );
    assert.equal(
      loaded.objectCount,
      expectedObjects,
      `${example.name}: browser object count after load`,
    );

    for (const [checkpointIndex, time] of sampleTimes(latestEnd).entries()) {
      const screenshotPath = path.join(
        artifactDir,
        artifactName(index, example.name, checkpointIndex + 1),
      );
      const { metrics, screenshot } = await renderAndCapture(page, time, screenshotPath);
      assert.equal(metrics.error, null, `${example.name}: browser runtime error`);
      assert.equal(metrics.revision, loaded.revision, `${example.name}: scene revision drifted`);
      assert.equal(metrics.objectCount, expectedObjects, `${example.name}: object count drifted`);
      assert.ok(Math.abs(metrics.time - time) < 1e-6, `${example.name}: deterministic seek time drifted`);
      assert.ok(metrics.drawCalls > 0, `${example.name}: renderer emitted no draw calls at t=${time}`);
      assert.ok(metrics.instances > 0, `${example.name}: renderer emitted no instances at t=${time}`);

      const { changedPixels: visiblePixels } = visiblePixelStats(screenshot, example.name);

      if (visiblePixels < 100) {
        visualFailures.push(
          `${example.name} @ ${time.toFixed(3)}s: ${visiblePixels} non-background pixels`,
        );
        console.log(
          `✗ ${example.name} @ ${time.toFixed(3)}s: ${metrics.drawCalls} draws, ` +
            `${metrics.instances} instances, ${visiblePixels} visible pixels`,
        );
      } else {
        console.log(
          `✓ ${example.name} @ ${time.toFixed(3)}s: ${metrics.objectCount} objects, ` +
            `${metrics.drawCalls} draws, ${metrics.instances} instances, ` +
            `${visiblePixels} visible pixels`,
        );
      }
    }

    if (example.name === "Filled path Transform") {
      const fillCheckpoints = [
        ["fill-start", 0],
        ["fill-mid", latestEnd * 0.5],
        ["fill-end", latestEnd],
      ];
      const centerColors = [];
      for (const [checkpoint, time] of fillCheckpoints) {
        const capture = await renderAndCapture(
          page,
          time,
          path.join(artifactDir, artifactName(index, example.name, checkpoint)),
        );
        assert.equal(capture.metrics.error, null, `${example.name}: ${checkpoint} runtime error`);
        const center = pixelAt(capture.screenshot, 0.5, 0.5);
        const background = pixelAt(capture.screenshot, 0, 0);
        const fillSeparation = pixelDistance(center, background);
        assert.ok(
          fillSeparation >= 64,
          `${example.name}: center fill disappeared at ${checkpoint} (${fillSeparation} channel-distance)`,
        );
        centerColors.push(center);
      }
      const endpointColorDelta = pixelDistance(centerColors[0], centerColors.at(-1));
      assert.ok(
        endpointColorDelta >= 48,
        `${example.name}: fill color failed to change across the transform (${endpointColorDelta} channel-distance)`,
      );
      console.log(
        `✓ ${example.name}: fill remains raster-visible and endpoint color changes (${endpointColorDelta} channel-distance)`,
      );
    }

    if (example.name === "Create shapes") {
      const beforeTime = latestEnd - 0.001;
      const beforePath = path.join(
        artifactDir,
        artifactName(index, example.name, "continuity-before"),
      );
      const afterPath = path.join(
        artifactDir,
        artifactName(index, example.name, "continuity-after"),
      );
      const before = await renderAndCapture(page, beforeTime, beforePath);
      const after = await renderAndCapture(page, latestEnd, afterPath);
      assert.equal(before.metrics.error, null, `${example.name}: pre-completion runtime error`);
      assert.equal(after.metrics.error, null, `${example.name}: completion runtime error`);

      const beforeStats = visiblePixelStats(before.screenshot, `${example.name} before completion`);
      const afterStats = visiblePixelStats(after.screenshot, `${example.name} at completion`);
      const boundDelta = Math.max(
        Math.abs(beforeStats.bounds.minX - afterStats.bounds.minX),
        Math.abs(beforeStats.bounds.minY - afterStats.bounds.minY),
        Math.abs(beforeStats.bounds.maxX - afterStats.bounds.maxX),
        Math.abs(beforeStats.bounds.maxY - afterStats.bounds.maxY),
      );
      assert.ok(
        boundDelta <= 1,
        `${example.name}: Create-to-analytic visible bounds jumped by ${boundDelta}px`,
      );
      console.log(`✓ ${example.name}: Create-to-analytic bounds continuous within ${boundDelta}px`);

      const lineBeforePath = path.join(
        artifactDir,
        artifactName(index, example.name, "line-end-before"),
      );
      const lineEndPath = path.join(
        artifactDir,
        artifactName(index, example.name, "line-end-final"),
      );
      const lineBefore = await renderAndCapture(page, latestEnd - 0.04, lineBeforePath);
      const lineEnd = await renderAndCapture(page, latestEnd, lineEndPath);
      const lineEndpointDiff = differingPixelCount(
        lineBefore.screenshot,
        lineEnd.screenshot,
        { minX: 0.64, maxX: 0.98, minY: 0.12, maxY: 0.48 },
      );
      assert.ok(
        lineEndpointDiff <= 24,
        `${example.name}: line endpoint changed across ${lineEndpointDiff} pixels near completion`,
      );
      console.log(
        `✓ ${example.name}: line endpoint continuous near completion (${lineEndpointDiff} changed pixels)`,
      );

      const waveBeforePath = path.join(
        artifactDir,
        artifactName(index, example.name, "wave-end-before"),
      );
      const waveEndPath = path.join(
        artifactDir,
        artifactName(index, example.name, "wave-end-final"),
      );
      const waveBefore = await renderAndCapture(page, latestEnd - 0.001, waveBeforePath);
      const waveEnd = await renderAndCapture(page, latestEnd, waveEndPath);
      const waveEndpointDiff = differingPixelCount(
        waveBefore.screenshot,
        waveEnd.screenshot,
        { minX: 0.58, maxX: 0.84, minY: 0.55, maxY: 0.96 },
      );
      assert.ok(
        waveEndpointDiff <= 20,
        `${example.name}: wave endpoint jumped across ${waveEndpointDiff} pixels at completion`,
      );
      console.log(
        `✓ ${example.name}: wave endpoint continuous at completion (${waveEndpointDiff} changed pixels)`,
      );
    }
  }

  assert.deepEqual(browserErrors, [], `browser emitted errors:\n${browserErrors.join("\n")}`);
  assert.deepEqual(
    visualFailures,
    [],
    `browser visual smoke failures:\n${visualFailures.join("\n")}`,
  );
  console.log(
    `Browser ${expectedRendererBackend} smoke passed for ${examples.length} internal renderer fixtures at four semantic checkpoints each; direct Rust/WASM execution presented ${directMetrics.drawCalls} draw calls on ${directMetrics.backend}; ${browserAuthoredManimCount} public source-equivalent Manim scenes are validated by the browser authoring corpus.`,
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
