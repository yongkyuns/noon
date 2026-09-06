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
        angle.increment_value(0.5).set_value(1.5)
        self.bind_rotation(square, angle)
        assert abs(angle.get_value() - 1.5) < 1e-9

        progress = self.value_tracker(0.0)
        self.bind_position(circle, progress, direction=RIGHT, offset=UP)
        assert abs(progress.get_value() - 0.0) < 1e-9

        self.play(
            angle.animate(run_time=2.0, rate_func=linear).set_value(3.5)
        )
        assert abs(angle.get_value() - 3.5) < 1e-9

        # Scene.play options override builder-local timing just like ordinary .animate.
        self.play(
            progress.animate(run_time=5.0, rate_func=linear).set_value(2.0),
            square.animate.shift(UP),
            run_time=1.0,
            rate_func=smooth,
        )
        assert abs(progress.get_value() - 2.0) < 1e-9

        try:
            angle.set_value(4.0)
            raise AssertionError("direct mutation after tracker timeline authoring must fail")
        except ValueError as error:
            assert "timeline animation is ambiguous" in str(error)
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
  assert.ok(reactive, "reactive graph should be present");
  assert.equal(reactive.signals.length, 3);
  assert.equal(reactive.bindings.length, 2);
  // Signal IDs are derived runtime/document identities. The source only
  // qualifies the typed tracker behavior and the exported binding shape.
  assert.ok(
    reactive.signals.some(
      (signal) => signal.source?.input?.scalar === 1.5,
    ),
    "rotation tracker source should be exported",
  );
  assert.ok(
    reactive.signals.some(
      (signal) => signal.source?.input?.scalar === 0,
    ),
    "position tracker source should be exported",
  );
  assert.deepEqual(
    reactive.bindings.map((binding) => binding.property).sort(),
    ["position", "rotation"],
  );

  assert.deepEqual(result.document.signal_tracks, [
    {
      signal: 0,
      from: 1.5,
      to: 3.5,
      timing: { start_time: 0, duration: 2, easing: "linear" },
    },
    {
      signal: 1,
      from: 0,
      to: 2,
      timing: { start_time: 2, duration: 1, easing: "smooth" },
    },
  ]);

  const transform = result.document.tracks.find(
    (track) => track.object === 0 && track.property === "transform",
  );
  assert.ok(transform, "mixed tracker/object play should preserve object animation");
  assert.equal(transform.timing.start_time, 2);
  assert.equal(transform.timing.duration, 1);
  assert.equal(transform.timing.easing, "smooth");

  console.log("reactive authoring smoke test passed");
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
