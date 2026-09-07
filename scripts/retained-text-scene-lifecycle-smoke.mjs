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
        self.live_execution()
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
    (source) => window.noonManimCompat.runLive(source),
    lifecycleSource,
  );
  assert.equal(result.duration, 3);
  assert.equal(result.metrics.objectCount, 1, "animate must reintroduce the cleared Text root");
  assert.ok(result.metrics.presentedFrames > 0, "shared Text lifecycle must render");
  assert.deepEqual(errors, [], `browser errors while testing retained Text lifecycle:\n${errors.join("\n")}`);

  console.log(
    "Retained Text Scene lifecycle smoke passed: delayed add, remove, re-add, clear, and animate reintroduction share one live semantic scene.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
