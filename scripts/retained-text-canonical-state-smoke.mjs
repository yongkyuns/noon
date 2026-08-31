import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4198;
const baseUrl = `http://127.0.0.1:${port}`;
const tolerance = 1e-6;

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
  throw new Error(`retained canonical-state smoke server did not start: ${lastError}\n${serverOutput}`);
}

function assertNear(actual, expected, label) {
  assert.ok(
    Math.abs(actual - expected) <= tolerance,
    `${label}: expected ${expected}, got ${actual}`,
  );
}

const source = `
from noon import *

class RetainedCanonicalState(Scene):
    def construct(self):
        label = Text("Canonical", font_size=48)

        self.play(label.animate.shift(RIGHT), run_time=0.5, rate_func=linear)
        assert label.get_center().x == 1.0

        label.shift(UP)
        assert label.get_center().x == 1.0
        assert label.get_center().y == 1.0

        self.play(label.animate.shift(RIGHT), run_time=0.5, rate_func=linear)
        assert label.get_center().x == 2.0
        assert label.get_center().y == 1.0

        label.rotate(PI / 4)
        label.set_opacity(0.4)
        self.wait(0.25)
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

  const result = await page.evaluate((pythonSource) => window.noonManimCompat.run(pythonSource), source);
  assert.equal(result.kind, "scene_document");
  assert.equal(result.document.objects.length, 0, "retained Text must not create legacy geometry");
  assert.ok(result.retainedDocument, "retained Text must emit a retained authoring document");
  assert.equal(result.retainedDocument.objects.length, 1);

  const object = result.retainedDocument.objects[0];
  assert.equal(object.text.source, "Canonical");
  assert.deepEqual(
    object.text.transform.translation,
    { x: 0, y: 0 },
    "later animations and direct edits must not rewrite the time-zero retained spec",
  );
  assert.deepEqual(object.text.transform.scale, { x: 1, y: 1 });
  assertNear(object.text.transform.rotation, 0, "time-zero rotation");
  assertNear(object.text.opacity, 1, "time-zero opacity");

  const tracks = result.retainedDocument.tracks ?? [];
  const position = tracks.filter((track) => track.property === "position");
  assert.equal(position.length, 3, "animate/direct/animate position history must stay explicit");
  assert.deepEqual(position[0].values.vec2, {
    from: { x: 0, y: 0 },
    to: { x: 1, y: 0 },
  });
  assert.equal(position[0].timing.start_time, 0);
  assert.equal(position[0].timing.duration, 0.5);
  assert.equal(position[0].timing.easing, "linear");

  assert.deepEqual(position[1].values.vec2, {
    from: { x: 1, y: 0 },
    to: { x: 1, y: 1 },
  });
  assert.equal(position[1].timing.start_time, 0.5);
  assert.equal(position[1].timing.duration, 0);
  assert.equal(position[1].timing.easing, "linear");

  assert.deepEqual(position[2].values.vec2, {
    from: { x: 1, y: 1 },
    to: { x: 2, y: 1 },
  });
  assert.equal(position[2].timing.start_time, 0.5);
  assert.equal(position[2].timing.duration, 0.5);
  assert.equal(position[2].timing.easing, "linear");

  const rotation = tracks.filter((track) => track.property === "rotation");
  assert.equal(rotation.length, 1);
  assertNear(rotation[0].values.scalar.from, 0, "direct rotation source");
  assertNear(rotation[0].values.scalar.to, Math.PI / 4, "direct rotation target");
  assert.equal(rotation[0].timing.start_time, 1);
  assert.equal(rotation[0].timing.duration, 0);

  const opacity = tracks.filter((track) => track.property === "opacity");
  assert.equal(opacity.length, 1);
  assertNear(opacity[0].values.scalar.from, 1, "direct opacity source");
  assertNear(opacity[0].values.scalar.to, 0.4, "direct opacity target");
  assert.equal(opacity[0].timing.start_time, 1);
  assert.equal(opacity[0].timing.duration, 0);

  assert.equal(result.duration, 1.25);
  assert.deepEqual(errors, []);
  console.log("retained Text canonical authoring state smoke passed");
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
