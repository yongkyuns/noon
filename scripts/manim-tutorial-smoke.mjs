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
assert.ok(ready.length >= 1, "expected source-equivalent Manim examples");

const parityManifestPath = path.join(repoRoot, "parity", "manim-v0.21", "manifest.json");
const parityManifest = JSON.parse(await readFile(parityManifestPath, "utf8"));
const parityFixtures = new Map(parityManifest.fixtures.map((fixture) => [fixture.id, fixture]));

const ids = new Set();
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
  assert.ok(entry.parity_fixture, `${entry.id}: runnable examples require a parity fixture`);
  assert.ok(
    parityFixtures.has(entry.parity_fixture),
    `${entry.id}: unknown parity fixture ${entry.parity_fixture}`,
  );
  assert.ok(entry.thumbnail, `${entry.id}: runnable examples require a static thumbnail`);
  await access(path.join(repoRoot, "web", entry.path));
  await access(path.join(repoRoot, "web", entry.thumbnail));
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
  assert.ok(tracks.length > 0, "tutorial must exercise timed behavior");
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

function assertDurationContract(entry, result) {
  // Scene.time is authoritative because no-op waits advance semantic time without
  // manufacturing renderer tracks. Source-equivalent gallery scenes are governed
  // by their canonical Manim fixture duration, not by an arbitrary playground loop.
  const fixture = parityFixtures.get(entry.parity_fixture);
  assert.ok(fixture, `${entry.id}: missing parity fixture`);
  const actual = sceneDuration(result);
  assert.ok(
    Math.abs(actual - fixture.expected_duration) <= 1e-9,
    `${entry.id}: expected parity duration ${fixture.expected_duration}, got ${actual}`,
  );
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
    const sourceUrl = `${baseUrl}/web/${entry.path}`;
    const response = await fetch(sourceUrl);
    assert.equal(response.ok, true, `${entry.id}: unable to fetch ${sourceUrl}`);
    const source = await response.text();
    const result = await page.evaluate(
      (pythonSource) => window.noonManimCompat.run(pythonSource),
      source,
    );
    assert.equal(result.kind, "scene_document", `${entry.id}: expected scene document`);
    assert.ok(result.document.objects.length > 0, `${entry.id}: expected scene objects`);
    assertDurationContract(entry, result);
    console.log(`[PASS] ${entry.id}`);
  }

  assert.equal(browserErrors.length, 0, browserErrors.join("\n"));
  console.log(`${ready.length}/${ready.length} source-equivalent Manim examples passed`);
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
