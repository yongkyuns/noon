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

const foundationSource = `
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

const phaseBSource = `
from noon import *

class GroupAndSceneMembership(Scene):
    def construct(self):
        left = Circle(radius=0.35, color=BLUE)
        right = Square(side_length=0.7, color=PINK)
        pair = VGroup(left, right).arrange(RIGHT, buff=0.4)

        assert isinstance(pair, Mobject)
        assert isinstance(pair, Group)
        self.add(pair)
        assert len(self.mobjects) == 1 and self.mobjects[0] is pair

        self.play(pair.animate.shift(UP).scale(0.8), run_time=0.5, rate_func=smooth)
        self.remove(pair)
        assert self.mobjects == []

        self.wait(0.1)
        self.add(pair)
        assert len(self.mobjects) == 1 and self.mobjects[0] is pair

        replacement = Circle(radius=0.2, color=GREEN)
        self.replace(pair, replacement)
        assert len(self.mobjects) == 1 and self.mobjects[0] is replacement

        # set_y is intentionally not one of Noon's old fixed animation-builder methods.
        self.play(replacement.animate.set_y(1.5), run_time=0.4, rate_func=linear)
        self.clear()
        assert self.mobjects == []

        intro = VGroup(
            Circle(radius=0.18, color=BLUE),
            Square(side_length=0.36, color=PINK),
        ).arrange(RIGHT, buff=0.2)
        self.play(FadeIn(intro), run_time=0.25)
        assert len(self.mobjects) == 1 and self.mobjects[0] is intro
        self.play(FadeOut(intro), run_time=0.25)
        assert self.mobjects == []
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

  const foundation = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    foundationSource,
  );
  assert.equal(foundation.kind, "scene_document");
  assert.equal(foundation.document.objects.length, 3, "introducer animations should auto-bind objects");

  const foundationProperties = foundation.document.tracks.map((track) => track.property);
  assert.equal(foundationProperties.filter((property) => property === "presence").length, 3);
  assert.equal(foundationProperties.filter((property) => property === "reveal").length, 2);
  assert.ok(foundationProperties.includes("transform"), "animate.shift should lower to transform");

  const revealTracks = foundation.document.tracks.filter((track) => track.property === "reveal");
  assert.ok(
    revealTracks.every((track) => track.timing.easing === "ease_in_out_cubic"),
    "rate_func=smooth should lower to deterministic ease_in_out_cubic",
  );
  const transform = foundation.document.tracks.find((track) => track.property === "transform");
  assert.equal(transform.timing.easing, "linear");

  const phaseB = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    phaseBSource,
  );
  assert.equal(phaseB.kind, "scene_document");
  assert.equal(phaseB.document.objects.length, 5, "groups should lower to flat runtime member objects");
  const phaseBProperties = phaseB.document.tracks.map((track) => track.property);
  assert.equal(
    phaseBProperties.filter((property) => property === "transform").length,
    3,
    "group animate should lower to member transforms and generic set_y should lower once",
  );
  assert.equal(
    phaseBProperties.filter((property) => property === "presence").length,
    12,
    "scene membership and grouped fades should lower to deterministic presence events",
  );

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
    "Manim compatibility smoke passed: construct discovery, shape classes, scene membership, group lowering, generic animate proxying, z=0 vectors, and rate_func lowering.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
