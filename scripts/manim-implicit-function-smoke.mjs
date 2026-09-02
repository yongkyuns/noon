import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4189;
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
  throw new Error(`ImplicitFunction smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *
import json
import math
import _manim_semantic_handles as _handles


def assert_close(actual, expected, tolerance=1e-4):
    assert abs(float(actual) - float(expected)) <= tolerance, (actual, expected)


def snapshot_for(mobject):
    handle = _handles._handle_for(mobject)
    assert handle is not None
    return json.loads(str(handle.snapshotJson()))


def path_commands(mobject):
    snapshot = snapshot_for(mobject)
    return snapshot, snapshot["geometry"]["vector_path"]["commands"]


def command_point(command):
    payload = command.get("move_to") or command.get("line_to") or command.get("cubic_to")
    assert payload is not None, command
    point = payload["to"]
    return Vec2(float(point["x"]), float(point["y"]))


class RetainedImplicitFunctionScene(Scene):
    def construct(self):
        circle_calls = []

        def circle_field(x, y):
            circle_calls.append((float(x), float(y)))
            return x * x + y * y - 1.0

        direct = ImplicitFunction(
            circle_field,
            x_range=[-1.5, 1.5],
            y_range=[-1.5, 1.5],
            min_depth=4,
            max_quads=512,
            use_smoothing=False,
            color=BLUE,
        )
        assert isinstance(direct, VMobject)
        assert direct.function is circle_field
        assert direct.x_range == [-1.5, 1.5]
        assert direct.y_range == [-1.5, 1.5]
        assert direct.min_depth == 4
        assert direct.max_quads == 512
        assert direct.use_smoothing is False
        assert len(circle_calls) > 0

        direct_snapshot, direct_commands = path_commands(direct)
        assert len(direct_commands) > 8
        assert all(
            "move_to" in command or "line_to" in command
            for command in direct_commands
        )
        first = command_point(direct_commands[0])
        last = command_point(direct_commands[-1])
        assert (first - last).length() <= 2e-3
        stroke = direct_snapshot["style"]["stroke"]
        assert_close(stroke["red"], BLUE.red)
        assert_close(stroke["green"], BLUE.green)
        assert_close(stroke["blue"], BLUE.blue)

        smooth = ImplicitFunction(
            lambda x, y: x * x + y * y - 1.0,
            x_range=[-1.5, 1.5],
            y_range=[-1.5, 1.5],
            min_depth=4,
            max_quads=512,
            color=GREEN,
        )
        _, smooth_commands = path_commands(smooth)
        assert any("cubic_to" in command for command in smooth_commands)
        assert not any("line_to" in command for command in smooth_commands)

        nan_field = ImplicitFunction(
            lambda x, y: math.nan if x < 0.0 else x - 0.5,
            x_range=[-1.0, 1.0],
            y_range=[-1.0, 1.0],
            min_depth=3,
            max_quads=128,
            use_smoothing=False,
        )
        _, nan_commands = path_commands(nan_field)
        assert len(nan_commands) >= 2
        for command in nan_commands:
            point = command_point(command)
            assert_close(point.x, 0.5, tolerance=5e-3)

        axes = Axes(
            x_range=[-2, 2, 1],
            y_range=[-2, 2, 1],
            x_length=8,
            y_length=4,
            tips=False,
        )
        axes.shift(RIGHT * 0.75 + UP * 0.25)
        axes_calls = []

        def diagonal_field(x, y):
            axes_calls.append((float(x), float(y)))
            return y - x

        plotted = axes.plot_implicit_curve(
            diagonal_field,
            min_depth=3,
            max_quads=128,
            use_smoothing=False,
            color=PURPLE,
        )
        plotted_snapshot, plotted_commands = path_commands(plotted)
        assert len(plotted_commands) >= 2
        transform = plotted_snapshot["transform"]
        for command in plotted_commands:
            local = command_point(command)
            world = Vec2(
                local.x * float(transform["scale"]["x"])
                    + float(transform["translation"]["x"]),
                local.y * float(transform["scale"]["y"])
                    + float(transform["translation"]["y"]),
            )
            x, y = axes.p2c(world)
            assert_close(y, x, tolerance=5e-3)
        assert len(axes_calls) > 0

        try:
            ImplicitFunction(
                lambda x, y: (_ for _ in ()).throw(RuntimeError("implicit boom")),
                x_range=[-1, 1],
                y_range=[-1, 1],
                min_depth=0,
                max_quads=1,
                use_smoothing=False,
            )
            raise AssertionError("implicit callback failure must propagate")
        except Exception as error:
            assert "implicit boom" in str(error)

        try:
            ImplicitFunction(
                lambda x, y: [x, y],
                x_range=[-1, 1],
                y_range=[-1, 1],
                min_depth=0,
                max_quads=1,
                use_smoothing=False,
            )
            raise AssertionError("nonscalar implicit callback must fail")
        except Exception as error:
            assert "real scalar" in str(error)

        direct_calls_before_add = len(circle_calls)
        axes_calls_before_add = len(axes_calls)
        self.add(direct, smooth, plotted)
        assert len(circle_calls) == direct_calls_before_add
        assert len(axes_calls) == axes_calls_before_add
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
  assert.equal(result.document.tracks.length, 0);
  assert.equal(result.document.objects.length, 3);
  assert.deepEqual(
    result.document.objects.map((object) => Object.keys(object.geometry)[0]),
    ["vector_path", "vector_path", "vector_path"],
    "implicit curves must lower to ordinary retained VectorPath geometry",
  );
  assert.deepEqual(
    errors,
    [],
    `browser errors while testing retained ImplicitFunction:\n${errors.join("\n")}`,
  );
  console.log(
    "Retained ImplicitFunction smoke passed: adaptive Rust-owned contours, smooth/unsmoothed paths, NaN boundaries, Axes mapping, callback error propagation, and zero per-frame Python work.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
