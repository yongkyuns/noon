import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4178;
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
  throw new Error(`Reactive authoring smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *

class NativeTrackers(Scene):
    def construct(self):
        square = Square(side_length=0.8, color=BLUE)
        circle = Circle(radius=0.3, color=PINK)
        self.add(square, circle)

        angle = ValueTracker(0.25)
        assert abs(angle.get_value() - 0.25) < 1e-9
        angle.increment_value(0.5).set_value(1.5)
        self.bind_rotation(square, angle)
        assert angle.signal_id == 0

        progress = self.value_tracker(2.0)
        self.bind_position(circle, progress, direction=RIGHT, offset=UP)
        assert progress.signal_id == 1

        try:
            _ = angle.animate
            raise AssertionError("timeline-driven ValueTracker animation must fail explicitly")
        except NotImplementedError as error:
            assert "timeline-driven signal tracks" in str(error)
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
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    source,
  );
  assert.equal(result.kind, "scene_document");
  assert.equal(errors.length, 0, errors.join("\n"));

  const reactive = result.document.reactive;
  assert.ok(reactive, "reactive graph should be included only when used");
  assert.equal(reactive.signals.length, 3);
  assert.equal(reactive.bindings.length, 2);

  assert.deepEqual(reactive.signals[0], {
    id: 0,
    source: { input: { scalar: 1.5 } },
  });
  assert.deepEqual(reactive.bindings[0], {
    signal: 0,
    object: 0,
    property: "rotation",
  });
  assert.deepEqual(reactive.signals[1], {
    id: 1,
    source: { input: { scalar: 2.0 } },
  });
  assert.deepEqual(reactive.signals[2], {
    id: 2,
    source: {
      derived: {
        add: [
          { constant: { vec2: { x: 0, y: 1 } } },
          {
            mul: [
              { signal: 1 },
              { constant: { vec2: { x: 1, y: 0 } } },
            ],
          },
        ],
      },
    },
  });
  assert.deepEqual(reactive.bindings[1], {
    signal: 2,
    object: 1,
    property: "position",
  });

  console.log("reactive authoring smoke test passed");
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
