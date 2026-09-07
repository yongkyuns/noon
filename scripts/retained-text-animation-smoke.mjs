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

const retainedAnimateSource = `
from noon import *

class RetainedAnimate(Scene):
    def construct(self):
        label = Text("Animate", font_size=48)

        self.play(
            label.animate(run_time=2.0, rate_func=linear)
                .scale(2.0)
                .shift(RIGHT)
                .rotate(PI / 2)
                .set_opacity(0.25)
        )
        assert abs(label.get_center()[0] - 1.0) < 1e-5
        assert abs(label.get_center()[1]) < 1e-5
        self.play(
            label.animate.scale(0.5).shift(UP).rotate(-PI / 4).set_opacity(0.75),
            run_time=1.0,
        )
        assert abs(label.get_center()[0] - 1.0) < 1e-5
        assert abs(label.get_center()[1] - 1.0) < 1e-5
        self.play(label.animate.move_to(2 * LEFT), run_time=1.0)
        assert abs(label.get_center()[0] + 2.0) < 1e-5
        assert abs(label.get_center()[1]) < 1e-5
        assert label in self.mobjects

        invalid = Text("Invalid", font_size=36)
        try:
            self.play(invalid.animate.rotate(float("nan")), run_time=0.25)
            raise AssertionError("non-finite Text animation must fail")
        except ValueError:
            pass
        assert invalid not in self.mobjects
        assert abs(invalid.get_center()[0]) < 1e-5
        assert abs(invalid.get_center()[1]) < 1e-5
`;

const retainedFadeSource = `
from noon import *

class RetainedFade(Scene):
    def construct(self):
        label = Text("Fade", font_size=48).shift(LEFT)

        self.wait(0.5)
        assert label not in self.mobjects

        self.play(
            FadeIn(label, shift=DOWN, scale=0.5),
            run_time=1.0,
            rate_func=linear,
        )
        assert label in self.mobjects

        self.play(
            FadeOut(label, shift=2 * RIGHT, scale=1.5),
            run_time=1.0,
        )
        assert label not in self.mobjects

        self.wait(0.5)
        self.add(label)
        assert label in self.mobjects
        self.play(label.animate.shift(UP), run_time=1.0)
        assert label in self.mobjects
        assert abs(label.get_center()[0] + 1.0) < 1e-5
        assert abs(label.get_center()[1] - 1.0) < 1e-5
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
    (sources) => window.noonManimCompat.runLiveSources(sources),
    [retainedAnimateSource, retainedFadeSource],
  );
  assert.equal(result.sameCanvas, true, "Text rerun must retain the mounted canvas");
  for (const [index, execution] of result.results.entries()) {
    assert.equal(execution.duration, 4, `Text case ${index}: authored duration`);
    assert.equal(execution.metrics.objectCount, 1, `Text case ${index}: final membership`);
    assert.ok(execution.metrics.presentedFrames > 0, `Text case ${index}: no rendered frame`);
  }
  assert.deepEqual(errors, [], `browser errors while testing retained Text animation:\n${errors.join("\n")}`);

  console.log(
    "Text animation smoke passed through shared live execution: scale, rotation, opacity, relative/absolute movement, FadeIn/FadeOut, re-add, and canvas reuse.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
