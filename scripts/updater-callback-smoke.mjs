import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4182;
const baseUrl = `http://127.0.0.1:${port}`;
const source = await readFile(
  path.join(repoRoot, "web/python/examples/perf_host_updater.py"),
  "utf8",
);

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
      const response = await fetch(`${baseUrl}/web/updater-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Updater smoke server did not start: ${lastError}\n${serverOutput}`);
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
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/updater-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonUpdaterSmoke, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonUpdaterSmoke.ready());

  const output = await page.evaluate(
    ({ pythonSource, times }) => window.noonUpdaterSmoke.run(pythonSource, times),
    { pythonSource: source, times: [0.25, 0.75] },
  );
  assert.equal(errors.length, 0, errors.join("\n"));
  assert.equal(output.result.kind, "scene_document");
  assert.ok(output.result.semanticExecution);
  assert.equal(output.paused.playing, false);
  assert.ok(output.paused.time <= 0.25);
  assert.equal(output.phases.length, 2);

  const [first, second] = output.phases;
  for (const phase of output.phases) {
    assert.equal(phase.schema_version, 1);
    assert.equal(phase.outcome, "presented");
    assert.equal(phase.committed.dirty, "updated");
    assert.equal(phase.committed.presence, true);
    assert.equal(phase.mirrored.object, phase.committed.object);
    assert.equal(phase.mirrored.frame_index, phase.committed.frame_index);
    assert.equal(phase.mirrored.time, phase.committed.time);
    assert.deepEqual(phase.mirrored.transform, phase.committed.transform);
    assert.deepEqual(phase.mirrored.style, phase.committed.style);
    assert.equal(phase.prepared.kind, "geometry");
    assert.ok(phase.upload.target_geometry_writes.length > 0);
    assert.equal(phase.presentation.presented, true);
  }

  assert.equal(first.committed.time, 0.25);
  assert.deepEqual(first.committed.transform.translation, { x: 2, y: 0.25 });
  assert.equal(first.committed.style.opacity, 0.75);
  assert.equal(second.committed.time, 0.75);
  assert.deepEqual(second.committed.transform.translation, { x: 2, y: 0.5 });
  assert.equal(second.committed.style.opacity, 1);
  assert.equal(second.committed.object, first.committed.object);
  assert.equal(second.committed.frame_index, first.committed.frame_index);
  assert.ok(second.publication.sequence > first.publication.sequence);

  assert.equal(output.state.time, 0.75);
  assert.equal(output.state.playing, false);
  assert.equal(output.metrics.metrics.objectCount, 2);
  assert.ok(output.metrics.metrics.drawCalls > 0);

  console.log("Canonical Python updater callback smoke test passed");
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
