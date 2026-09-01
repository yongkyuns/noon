import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4186;
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
  throw new Error(`NumberLine smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *
import json
import math
import _manim_semantic_handles as _handles


def assert_close(actual, expected, tolerance=1e-5):
    assert abs(float(actual) - float(expected)) <= tolerance, (actual, expected)


def assert_point_close(actual, expected, tolerance=1e-5):
    assert_close(actual.x, expected.x, tolerance)
    assert_close(actual.y, expected.y, tolerance)


class RetainedNumberLineScene(Scene):
    def construct(self):
        assert issubclass(UnitInterval, NumberLine)

        default_line = NumberLine()
        assert default_line.x_range == [-7.0, 7.0, 1.0]
        assert default_line.length is None
        assert_close(default_line.unit_size, 1.0)
        assert len(default_line.ticks) == 15
        assert_point_close(default_line.n2p(0.0), ORIGIN)

        line = NumberLine(
            x_range=[-2, 2, 1],
            length=8,
            color=RED,
            numbers_with_elongated_ticks=[-1, 1],
        )
        assert isinstance(line, VGroup)
        assert line.x_range == [-2.0, 2.0, 1.0]
        assert_close(line.length, 8.0)
        assert_close(line.unit_size, 2.0)
        assert len(line.get_tick_marks()) == 5
        assert_close((line.ticks[1].get_end() - line.ticks[1].get_start()).length(), 0.4)
        assert_close((line.ticks[2].get_end() - line.ticks[2].get_start()).length(), 0.2)
        line_snapshot = json.loads(str(_handles._handle_for(line._line).snapshotJson()))
        assert_close(line_snapshot["style"]["stroke"]["red"], RED.red)

        assert_point_close(line @ 1.0, line.n2p(1.0))
        assert_close((line.n2p(-0.5)) @ line, -0.5)
        initial = line.n2p(1.25)
        assert_close(line.p2n(initial), 1.25)

        line.shift(LEFT * 0.7 + UP * 0.4)
        line.scale(0.8)
        line.rotate(0.35)
        transformed = line.n2p(1.25)
        assert_close(line.p2n(transformed), 1.25)
        assert (transformed - initial).length() > 0.1

        copied = line.copy()
        assert isinstance(copied, NumberLine)
        assert copied is not line
        assert copied._line is not line._line
        assert_point_close(copied.n2p(1.25), transformed)
        copied.shift(RIGHT)
        assert (copied.n2p(1.25) - line.n2p(1.25)).length() > 0.5

        derived = NumberLine(x_range=[0, 4, 1], unit_size=1.5, length=0)
        assert_close(derived.length, 0.0)
        assert_close(derived.unit_size, 1.5)
        assert_close((derived.get_end() - derived.get_start()).length(), 6.0)

        interval = UnitInterval()
        assert interval.x_range == [0.0, 1.0, 0.1]
        assert_close(interval.unit_size, 10.0)
        assert interval.decimal_number_config == {"num_decimal_places": 1}
        assert interval.numbers_with_elongated_ticks == [0.0, 1.0]
        assert len(interval.ticks) == 11
        assert_close((interval.ticks[0].get_end() - interval.ticks[0].get_start()).length(), 0.4)
        assert_close((interval.ticks[-1].get_end() - interval.ticks[-1].get_start()).length(), 0.4)

        axes = Axes(
            x_range=[-2, 2, 1],
            y_range=[-2, 2, 1],
            x_length=4,
            y_length=4,
            tips=False,
        )
        assert isinstance(axes.x_axis, NumberLine)
        assert isinstance(axes.y_axis, NumberLine)
        assert_point_close(axes.x_axis.n2p(1.0), axes.c2p(1.0, 0.0))
        assert_point_close(axes.y_axis.n2p(1.0), axes.c2p(0.0, 1.0))
        axes.shift(DOWN * 0.5)
        axes.rotate(-0.2)
        axis_point = axes.x_axis.n2p(1.5)
        assert_close(axes.x_axis.p2n(axis_point), 1.5)

        self.add(line, derived, interval, axes)
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
  assert.equal(result.document.objects.length, 34);
  assert.equal(result.document.tracks.length, 0);
  assert.equal(
    result.document.objects.filter((object) => Object.keys(object.geometry)[0] === "line").length,
    34,
    "NumberLine, UnitInterval, and Axes must flatten only to ordinary retained Line geometry",
  );
  assert.deepEqual(errors, [], `browser errors while testing retained NumberLine:\n${errors.join("\n")}`);
  console.log(
    "Retained NumberLine smoke passed: defaults, length/unit-size resolution, shared ticks, transform-safe scalar queries, copy, matmul, Axes axis facade identity, and UnitInterval.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
