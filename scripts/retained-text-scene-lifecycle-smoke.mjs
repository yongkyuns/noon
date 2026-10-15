import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { browserArgs } from "./manim-raster-support.mjs";
import { serveRepository } from "./browser-test-server.mjs";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4192;
const server = await serveRepository(repoRoot, port, { crossOriginIsolated: true });
const baseUrl = server.baseUrl;

const lifecycleSource = `
from noon import *

class TextSceneLifecycle(Scene):
    def construct(self):
        label = Text("Lifecycle", font_size=48)
        self.live_execution()

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
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs("webgpu"),
  });
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
  // The source runner owns the test lifecycle. Do not pre-run unrelated
  // animated readiness probes on the same long-lived Python worker.

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
  await server.close();
}
