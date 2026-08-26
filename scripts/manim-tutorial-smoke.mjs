import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
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
assert.ok(ready.length >= 7, "expected the initial tutorial tranche");

const demoMainPath = path.join(repoRoot, "web", "main.js");
const demoMainSource = await readFile(demoMainPath, "utf8");
for (const entry of ready) {
  const pickerPath = `./${entry.path}`;
  assert.ok(
    demoMainSource.includes(`path: "${pickerPath}"`),
    `${entry.id}: ready tutorial is not exposed in the demo picker`,
  );
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
  const tracks = [
    ...(document.tracks ?? []),
    ...(document.signal_tracks ?? []),
  ];
  assert.ok(tracks.length > 0, "tutorial must exercise timed behavior");
  return Math.max(
    ...tracks.map(
      (track) => track.timing.start_time + track.timing.duration,
    ),
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
    assert.ok(latestEnd(result.document) < 4.0, `${entry.id}: exceeds interactive loop`);
    console.log(`[PASS] ${entry.id}`);
  }

  assert.equal(browserErrors.length, 0, browserErrors.join("\n"));
  console.log(`${ready.length}/${ready.length} tutorial examples passed`);
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
