import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4193;
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
  throw new Error(`retained family fade smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *

class RetainedFamilyFade(Scene):
    def construct(self):
        first = Text("Family A", font_size=40).shift(LEFT)
        second = Text("Family B", font_size=40).shift(RIGHT)
        family = VGroup(first, VGroup(second))

        mirror = family.copy()
        assert len(mirror) == 2

        detached = Text("Detached", font_size=32)
        holder = VGroup(detached)
        holder.remove(detached)
        holder.add(detached)

        unsupported = VGroup(
            Text("Unsupported A", font_size=32),
            Text("Unsupported B", font_size=32),
        )

        self.live_execution()
        self.wait(0.25)
        assert family not in self.mobjects

        self.play(FadeIn(family), run_time=0.75, rate_func=linear)
        assert family in self.mobjects
        assert first not in self.mobjects
        assert second not in self.mobjects

        self.play(FadeOut(family), run_time=0.5)
        assert family not in self.mobjects
        assert first not in self.mobjects
        assert second not in self.mobjects

        try:
            self.play(FadeIn(unsupported, shift=UP), run_time=0.25)
            raise AssertionError("shifted retained family FadeIn must fail")
        except NotImplementedError as error:
            assert "does not support shift, scale, or target_position" in str(error)
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

  const result = await page.evaluate((python) => window.noonManimCompat.runLive(python), source);
  assert.equal(result.duration, 1.5);
  assert.equal(result.metrics.objectCount, 0, "FadeOut must remove the family root");
  assert.ok(result.metrics.presentedFrames > 0, "shared family fades must render");
  assert.deepEqual(errors, [], `browser errors while testing retained family fades:\n${errors.join("\n")}`);
  console.log(
    "Retained Text family fade smoke passed: nested VGroup identity is shared, FadeIn/FadeOut execute through the shared runtime, wrapper identity stays top-level, and unsupported family layout endpoints fail before mutation.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
