import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4191;
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
  throw new Error(`retained text animation smoke server did not start: ${lastError}\n${serverOutput}`);
}

const retainedAnimateScaleSource = `
from noon import *

class RetainedAnimateScale(Scene):
    def construct(self):
        label = Text("Animate", font_size=48)

        self.play(
            label.animate(run_time=2.0, rate_func=linear).scale(2.0)
        )
        self.play(label.animate.scale(0.5), run_time=1.0)

        assert label in self.mobjects

        unsupported = Text("Unsupported", font_size=36)
        try:
            self.play(unsupported.animate.shift(RIGHT), run_time=0.25)
            raise AssertionError("unsupported retained animate.shift must fail")
        except NotImplementedError as error:
            assert "uniform scale only" in str(error)
        assert unsupported not in self.mobjects
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
    retainedAnimateScaleSource,
  );

  assert.equal(result.kind, "scene_document");
  assert.equal(
    result.document.objects.length,
    0,
    "retained Text.animate.scale must not create legacy placeholder geometry",
  );
  assert.ok(result.retainedDocument, "retained animation must emit a retained authoring document");
  assert.equal(result.retainedDocument.objects.length, 1);
  assert.equal(result.retainedDocument.objects[0].text.source, "Animate");

  const tracks = result.retainedDocument.tracks ?? [];
  assert.equal(tracks.length, 2, "two sequential animate.scale calls must emit two retained tracks");
  assert.ok(tracks.every((track) => track.property === "scale"));

  assert.deepEqual(tracks[0].values.vec2, {
    from: { x: 1, y: 1 },
    to: { x: 2, y: 2 },
  });
  assert.equal(tracks[0].timing.start_time, 0);
  assert.equal(tracks[0].timing.duration, 2);
  assert.equal(tracks[0].timing.easing, "linear");

  assert.deepEqual(tracks[1].values.vec2, {
    from: { x: 2, y: 2 },
    to: { x: 1, y: 1 },
  });
  assert.equal(tracks[1].timing.start_time, 2);
  assert.equal(tracks[1].timing.duration, 1);
  assert.equal(tracks[1].timing.easing, "smooth");
  assert.equal(result.duration, 3);

  const wire = JSON.stringify(result.retainedDocument);
  for (const forbidden of ["glyph", "font_bytes", "svg", "geometry", "atlas"]) {
    assert.ok(!wire.includes(forbidden), `retained animation wire must not contain ${forbidden}`);
  }
  assert.deepEqual(errors, [], `browser errors while testing retained Text animation:\n${errors.join("\n")}`);

  console.log(
    "Retained Text animation smoke passed: native Text.animate.scale lowers to source-level retained scale tracks, composes sequential relative scales, preserves timing options, and rejects unsupported retained target-state properties without legacy geometry.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
