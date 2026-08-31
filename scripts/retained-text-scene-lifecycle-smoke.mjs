import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4192;
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
  throw new Error(`retained Text lifecycle smoke server did not start: ${lastError}\n${serverOutput}`);
}

const lifecycleSource = `
from noon import *

class RetainedSceneLifecycle(Scene):
    def construct(self):
        label = Text("Lifecycle", font_size=48)

        self.wait(0.5)
        assert label not in self.mobjects
        self.add(label)
        assert label in self.mobjects

        self.wait(0.5)
        self.remove(label)
        assert label not in self.mobjects

        self.wait(0.5)
        self.add(label)
        assert label in self.mobjects

        self.wait(0.5)
        self.clear()
        assert label not in self.mobjects

        self.wait(0.5)
        self.play(label.animate.shift(UP), run_time=0.5)
        assert label in self.mobjects
`;

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

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonManimCompat.ready());

  const result = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    lifecycleSource,
  );
  assert.equal(result.kind, "scene_document");
  assert.equal(
    result.document.objects.length,
    0,
    "retained Scene lifecycle must not synthesize legacy placeholder geometry",
  );
  assert.ok(result.retainedDocument, "retained Scene lifecycle must emit a retained document");
  assert.equal(result.retainedDocument.objects.length, 1);
  assert.equal(result.retainedDocument.objects[0].text.source, "Lifecycle");
  assert.deepEqual(result.retainedDocument.objects[0].text.transform.translation, {
    x: 0,
    y: 0,
  });

  const tracks = result.retainedDocument.tracks ?? [];
  const presence = tracks.filter((track) => track.property === "presence");
  const position = tracks.filter((track) => track.property === "position");

  assert.deepEqual(
    presence.map((track) => ({
      values: track.values.bool,
      start: track.timing.start_time,
      duration: track.timing.duration,
      easing: track.timing.easing,
    })),
    [
      { values: { from: false, to: true }, start: 0.5, duration: 0, easing: "linear" },
      { values: { from: true, to: false }, start: 1, duration: 0, easing: "linear" },
      { values: { from: false, to: true }, start: 1.5, duration: 0, easing: "linear" },
      { values: { from: true, to: false }, start: 2, duration: 0, easing: "linear" },
      { values: { from: false, to: true }, start: 2.5, duration: 0, easing: "linear" },
    ],
    "direct add/remove/clear and animate reintroduction must share one Presence timeline",
  );

  assert.equal(position.length, 1);
  assert.deepEqual(position[0].values.vec2, {
    from: { x: 0, y: 0 },
    to: { x: 0, y: 1 },
  });
  assert.equal(position[0].timing.start_time, 2.5);
  assert.equal(position[0].timing.duration, 0.5);
  assert.equal(position[0].timing.easing, "smooth");

  assert.equal(result.duration, 3);
  const wire = JSON.stringify(result.retainedDocument);
  for (const forbidden of ["glyph", "font_bytes", "svg", "geometry", "atlas"]) {
    assert.ok(!wire.includes(forbidden), `retained lifecycle wire must not contain ${forbidden}`);
  }
  assert.deepEqual(errors, [], `browser errors while testing retained Text lifecycle:\n${errors.join("\n")}`);

  console.log(
    "Retained Text Scene lifecycle smoke passed: delayed add, remove, re-add, clear, and animate reintroduction share retained Presence state without placeholder geometry.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
