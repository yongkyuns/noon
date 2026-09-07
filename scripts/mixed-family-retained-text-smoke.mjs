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
  throw new Error(`mixed retained family smoke server did not start: ${lastError}\n${serverOutput}`);
}

const propertyFirstSource = `
from noon import *

class MixedRetainedPropertyFirst(Scene):
    def construct(self):
        moving = Text("MOVE")
        writing = Text("WRITE")
        self.play(
            moving.animate.shift(RIGHT),
            Write(writing),
            run_time=1.0,
            rate_func=linear,
        )
        assert self.mobjects == [moving, writing]
        assert abs(moving.get_center()[0] - 1) < 1e-6
`;

const editedFamilyFirstSource = `
from noon import *

class MixedRetainedFamilyFirst(Scene):
    def construct(self):
        writing = Text("EDITED")
        moving = Text("SHIFT")
        appearing = Text("FADE")
        self.play(
            Write(writing),
            moving.animate.shift(UP),
            FadeIn(appearing),
            run_time=1.0,
            rate_func=linear,
        )
        assert self.mobjects == [writing, moving, appearing]
        assert abs(moving.get_center()[1] - 1) < 1e-6
`;

const sameLeafSource = `
from noon import *

class MixedRetainedSameLeaf(Scene):
    def construct(self):
        label = Text("ONE")
        try:
            self.play(
                Write(label),
                label.animate.shift(RIGHT),
                run_time=0.25,
            )
            raise AssertionError("same-leaf family/property ownership must fail")
        except ValueError:
            pass
        assert len(self.mobjects) == 0
        assert abs(label.get_center()[0]) < 1e-6
        self.play(Write(label), run_time=0.25, rate_func=linear)
`;

const rollbackSource = `
from noon import *

class MixedRetainedRollback(Scene):
    def construct(self):
        writing = Text("ROLLBACK")
        moving = Text("MOVE")
        try:
            self.play(
                Write(writing),
                moving.animate(run_time=-1.0).shift(RIGHT),
            )
            raise AssertionError("negative run_time must fail")
        except ValueError:
            pass
        assert len(self.mobjects) == 0
        assert abs(moving.get_center()[0]) < 1e-6
        self.play(
            Write(writing),
            moving.animate.shift(RIGHT),
            run_time=0.5,
            rate_func=linear,
        )
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

  // Source reruns reuse one execution owner and canvas. Assertions in the
  // source inspect completed shared state and atomic failure through public APIs.
  const observed = await page.evaluate(
    (sources) => window.noonManimCompat.runLiveSources(sources),
    [propertyFirstSource, editedFamilyFirstSource, sameLeafSource, rollbackSource],
  );
  assert.equal(observed.sameCanvas, true, "live source reruns preserve the canvas");
  assert.deepEqual(observed.results.map((result) => result.duration), [1, 1, 0.25, 0.5]);
  assert.deepEqual(observed.results.map((result) => result.metrics.objectCount), [2, 3, 1, 2]);
  for (const result of observed.results) {
    assert.ok(result.metrics.presentedFrames > 0, "shared Text Write must present");
  }
  assert.deepEqual(errors, [], `browser errors while testing shared Text Write:\n${errors.join("\n")}`);
  console.log("Shared Text Write/property composition passed, including atomic rejection and source reruns.");
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
