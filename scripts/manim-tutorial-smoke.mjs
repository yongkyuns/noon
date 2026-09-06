import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const manifestPath = path.join(
  repoRoot,
  "web",
  "python",
  "examples",
  "manim_tutorial_manifest.json",
);
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const ready = manifest.entries.filter((entry) => entry.status === "ready");
assert.ok(ready.length >= 1, "expected exact-source Manim examples");

const parityManifestPath = path.join(repoRoot, "parity", "manim-v0.21", "manifest.json");
const parityManifest = JSON.parse(await readFile(parityManifestPath, "utf8"));
const parityFixtures = new Map(parityManifest.fixtures.map((fixture) => [fixture.id, fixture]));
const retainedTextAnimationId = "manim-shrink-to-center-text";

function noonSourceFromUpstream(source, id) {
  const upstreamImport = "from manim import *";
  const noonImport = "from noon import *";
  const occurrences = source.split(upstreamImport).length - 1;
  assert.equal(
    occurrences,
    1,
    `${id}: canonical upstream source must contain exactly one '${upstreamImport}'`,
  );
  return source.replace(upstreamImport, noonImport);
}

const ids = new Set();
const readySources = new Map();
for (const entry of manifest.entries) {
  assert.ok(!ids.has(entry.id), `${entry.id}: duplicate manifest id`);
  ids.add(entry.id);
}
for (const entry of ready) {
  assert.equal(
    entry.reuse,
    "source-equivalent-manim-v0.21",
    `${entry.id}: every runnable public example must be source-equivalent ManimCE v0.21`,
  );
  assert.ok(
    entry.parity_status === "candidate" || entry.parity_status === "parity-qualified",
    `${entry.id}: runnable examples require explicit parity status`,
  );
  if (entry.parity_status === "parity-qualified") {
    assert.ok(entry.parity_fixture, `${entry.id}: parity-qualified examples require a parity fixture`);
  }
  if (entry.parity_fixture) {
    assert.ok(
      parityFixtures.has(entry.parity_fixture),
      `${entry.id}: unknown parity fixture ${entry.parity_fixture}`,
    );
  }
  assert.ok(
    entry.parity_fixture || Number.isFinite(Number(entry.expected_duration)),
    `${entry.id}: candidate examples without a raster fixture require expected_duration`,
  );
  assert.ok(entry.thumbnail, `${entry.id}: runnable examples require a static thumbnail`);
  assert.ok(entry.upstream_source, `${entry.id}: runnable examples require canonical upstream source`);

  const publicPath = path.join(repoRoot, "web", entry.path);
  const upstreamPath = path.join(repoRoot, entry.upstream_source);
  await access(publicPath);
  await access(upstreamPath);
  await access(path.join(repoRoot, "web", entry.thumbnail));

  const [publicSource, upstreamSource] = await Promise.all([
    readFile(publicPath, "utf8"),
    readFile(upstreamPath, "utf8"),
  ]);
  assert.equal(
    publicSource,
    noonSourceFromUpstream(upstreamSource, entry.id),
    `${entry.id}: public source must match ManimCE v0.21 byte-for-byte except 'from manim import *' -> 'from noon import *'`,
  );
  readySources.set(entry.id, publicSource);
}

const port = 4182;
const baseUrl = `http://127.0.0.1:${port}`;
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
      const response = await fetch(`${baseUrl}/web/manim-compat-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Tutorial smoke server did not start: ${lastError}\n${serverOutput}`);
}

function latestEnd(document) {
  const tracks = [...(document.tracks ?? []), ...(document.signal_tracks ?? [])];
  if (tracks.length === 0) return 0;
  return Math.max(...tracks.map((track) => track.timing.start_time + track.timing.duration));
}

function sceneDuration(result) {
  const duration = Number(result.duration);
  assert.ok(
    Number.isFinite(duration) && duration >= 0,
    "authoring result must expose finite non-negative Scene time",
  );
  const trackEnd = latestEnd(result.document);
  assert.ok(
    duration + 1e-9 >= trackEnd,
    `Scene time ${duration} precedes latest emitted track end ${trackEnd}`,
  );
  return duration;
}

function expectedDuration(entry) {
  if (Number.isFinite(Number(entry.expected_duration))) {
    return Number(entry.expected_duration);
  }
  const fixture = parityFixtures.get(entry.parity_fixture);
  assert.ok(fixture, `${entry.id}: missing parity fixture`);
  return Number(fixture.expected_duration);
}

function assertDurationContract(entry, result) {
  const actual = sceneDuration(result);
  const expected = expectedDuration(entry);
  assert.ok(
    Math.abs(actual - expected) <= 1e-9,
    `${entry.id}: expected duration ${expected}, got ${actual}`,
  );
}

function authoredObjectCount(entry, result, retained) {
  const geometryCount = result.document.objects.length;
  const expectsRetainedText = entry.features?.includes("retained-text") ?? false;

  if (expectsRetainedText) {
    assert.equal(
      geometryCount,
      0,
      `${entry.id}: retained text must not create placeholder geometry`,
    );
    assert.ok(retained, `${entry.id}: retained text requires a retained authoring document`);
  }

  if (retained == null) return geometryCount;

  assert.equal(
    retained.channel,
    "noon.authoring.retained",
    `${entry.id}: retained objects require the canonical retained authoring channel`,
  );
  assert.equal(
    retained.protocol_version,
    2,
    `${entry.id}: retained objects require protocol v2`,
  );
  assert.ok(
    Array.isArray(retained.objects),
    `${entry.id}: retained authoring document requires an object list`,
  );
  return geometryCount + retained.objects.length;
}

function assertRetainedShrinkTrack(result, retained) {
  assert.equal(
    result.document.objects.length,
    0,
    `${retainedTextAnimationId}: animated retained Text must emit zero legacy geometry`,
  );
  assert.ok(retained, `${retainedTextAnimationId}: retained sidecar is required`);
  assert.equal(retained.channel, "noon.authoring.retained");
  assert.equal(retained.protocol_version, 2);
  assert.equal(retained.objects.length, 1);
  assert.equal(retained.objects[0].text.source, "Hello World!");
  assert.ok(Array.isArray(retained.tracks), `${retainedTextAnimationId}: retained tracks are required`);
  assert.equal(retained.tracks.length, 1);
  const track = retained.tracks[0];
  assert.equal(track.object, retained.objects[0].object);
  assert.equal(track.property, "scale");
  assert.deepEqual(track.values, {
    vec2: {
      from: { x: 1, y: 1 },
      to: { x: 0, y: 0 },
    },
  });
  assert.deepEqual(track.timing, {
    start_time: 0,
    duration: 1,
    easing: "smooth",
  });
  assert.equal(sceneDuration(result), 1);
}

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: ["--disable-dev-shm-usage"],
  });
  const page = await browser.newPage();
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonManimCompat.ready());

  for (const entry of ready) {
    const source = readySources.get(entry.id);
    assert.ok(source, `${entry.id}: exact source was not loaded`);
    const result = await page.evaluate(
      (pythonSource) => window.noonManimCompat.run(pythonSource),
      source,
    );
    assert.equal(result.kind, "scene_document", `${entry.id}: expected scene document`);
    const retained = await page.evaluate(
      ({ result, label }) => window.noonManimCompat.retainedTextView(result, label),
      { result, label: entry.id },
    );
    assert.ok(authoredObjectCount(entry, result, retained) > 0, `${entry.id}: expected scene objects`);
    assertDurationContract(entry, result);
    if (["parity-create-circle", "parity-square-to-circle"].includes(entry.id)) {
      const reveal = result.document.tracks.find((track) => track.property === "reveal");
      assert.ok(reveal, `${entry.id}: explicit export lost its Create reveal track`);
      assert.deepEqual(reveal.values, { scalar: { from: 0, to: 1 } });
      assert.equal(reveal.timing.start_time, 0);
      assert.equal(reveal.timing.duration, 1);
    }
    if (entry.id === retainedTextAnimationId) {
      assertRetainedShrinkTrack(result, retained);
    }
    console.log(`[PASS] ${entry.id}`);
  }

  assert.equal(browserErrors.length, 0, browserErrors.join("\n"));
  console.log(`${ready.length}/${ready.length} exact-source Manim examples passed`);
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
