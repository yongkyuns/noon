import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4182;
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
      const response = await fetch(`${baseUrl}/web/updater-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Updater smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *

class ArbitraryUpdaterScene(Scene):
    def construct(self):
        anchor = Circle(radius=0.4, color=RED).shift(RIGHT * 2)
        follower = Square(side_length=0.5, color=BLUE).shift(LEFT)

        def removed(mobject):
            mobject.shift(LEFT * 99)

        def follow(mobject, dt):
            # This deliberately reads another mobject to verify the callback phase
            # is a coherent scene snapshot rather than a target-only proxy.
            mobject.move_to(anchor.get_center() + UP * dt)
            mobject.set_opacity(0.5 + dt)

        follower.add_updater(removed)
        follower.remove_updater(removed)
        follower.add_updater(follow)
        assert follower.has_updaters()
        assert follower.get_updaters() == [follow]
        self.add(anchor, follower)
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

  await page.goto(`${baseUrl}/web/updater-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonUpdaterSmoke, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonUpdaterSmoke.ready());

  const output = await page.evaluate(
    ({ pythonSource, times }) => window.noonUpdaterSmoke.run(pythonSource, times),
    { pythonSource: source, times: [0.25, 0.75] },
  );
  assert.equal(errors.length, 0, errors.join("\n"));
  assert.equal(output.result.kind, "scene_document");
  assert.ok(output.result.callbacks);
  assert.deepEqual(output.result.callbacks.slots, [{ id: 0, objects: [0, 1] }]);
  assert.equal(output.phases.length, 2);
  assert.equal(output.nextSequence, 2);

  assert.equal(output.phases[0].frame.time, 0.25);
  assert.equal(output.phases[0].frame.delta_time, 0.25);
  assert.equal(output.phases[0].frame.objects.length, 2);
  assert.equal(output.phases[0].batch.sequence, 0);
  assert.equal(output.phases[0].batch.patches.length, 2);
  const firstTransform = output.phases[0].batch.patches[0].set_transform;
  assert.equal(firstTransform.object, 1);
  assert.equal(firstTransform.transform.translation.x, 2);
  assert.equal(firstTransform.transform.translation.y, 0.25);
  assert.equal(output.phases[0].batch.patches[1].set_style.style.opacity, 0.75);

  assert.equal(output.phases[1].frame.time, 0.75);
  assert.equal(output.phases[1].frame.delta_time, 0.5);
  assert.equal(output.phases[1].frame.objects[1].transform.translation.x, 2);
  assert.equal(output.phases[1].frame.objects[1].transform.translation.y, 0.25);
  assert.equal(output.phases[1].batch.sequence, 1);
  const secondTransform = output.phases[1].batch.patches[0].set_transform;
  assert.equal(secondTransform.transform.translation.x, 2);
  assert.equal(secondTransform.transform.translation.y, 0.5);
  assert.equal(output.phases[1].batch.patches[1].set_style.style.opacity, 1);

  assert.equal(output.finalFrame.time, 0.75);
  assert.equal(output.finalFrame.objects[1].transform.translation.x, 2);
  assert.equal(output.finalFrame.objects[1].transform.translation.y, 0.5);
  assert.equal(output.finalFrame.objects[1].style.opacity, 1);

  console.log("Python updater callback smoke test passed");
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
