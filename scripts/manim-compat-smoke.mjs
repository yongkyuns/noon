import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4175;
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
  throw new Error(`Manim compatibility smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *

class Demo(Scene):
    def construct(self):
        circle = Circle(radius=0.6, color=BLUE)
        square = Square(side_length=1.0, color=PINK).next_to(circle, RIGHT)
        assert isinstance(circle, Circle)
        assert isinstance(circle, VMobject)
        assert type(circle.copy()) is Circle

        self.play(
            Create(circle),
            Create(square),
            run_time=1.25,
            rate_func=smooth,
        )
        self.play(
            circle.animate.shift((0.0, 1.0, 0.0)),
            run_time=0.75,
            rate_func=linear,
        )
        self.play(FadeIn(Circle(radius=0.2, color=GREEN)), run_time=0.25)
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
  assert.equal(result.document.objects.length, 3, "introducer animations should auto-bind objects");

  const properties = result.document.tracks.map((track) => track.property);
  assert.equal(properties.filter((property) => property === "presence").length, 3);
  assert.equal(properties.filter((property) => property === "reveal").length, 2);
  assert.ok(properties.includes("transform"), "animate.shift should lower to transform");

  const revealTracks = result.document.tracks.filter((track) => track.property === "reveal");
  assert.ok(
    revealTracks.every((track) => track.timing.easing === "ease_in_out_cubic"),
    "rate_func=smooth should lower to deterministic ease_in_out_cubic",
  );
  const transform = result.document.tracks.find((track) => track.property === "transform");
  assert.equal(transform.timing.easing, "linear");

  let zError = null;
  try {
    await page.evaluate(
      (pythonSource) => window.noonManimCompat.run(pythonSource),
      `from noon import *\nresult = Scene()\nLine((0, 0, 1), (1, 0, 0))`,
    );
  } catch (error) {
    zError = String(error);
  }
  assert.match(zError ?? "", /z must be 0/, "non-zero z should fail explicitly");

  assert.deepEqual(errors, [], `browser errors while testing Manim compatibility:\n${errors.join("\n")}`);
  console.log(
    "Manim compatibility smoke passed: Scene.construct discovery, real shape classes, detached introducers, z=0 vectors, and rate_func lowering.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
