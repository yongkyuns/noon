import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4185;
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
  throw new Error(`Axes smoke server did not start: ${lastError}\n${serverOutput}`);
}

const axesSource = `
from noon import *
import json
import math
import _manim_semantic_handles as _handles


def assert_close(actual, expected, tolerance=1e-5):
    assert abs(float(actual) - float(expected)) <= tolerance, (actual, expected)


def assert_point_close(actual, expected, tolerance=1e-5):
    assert_close(actual.x, expected.x, tolerance)
    assert_close(actual.y, expected.y, tolerance)


def assert_three_point_graph_matches_axes(axes, graph):
    handle = _handles._handle_for(graph)
    assert handle is not None
    snapshot = json.loads(str(handle.snapshotJson()))
    commands = snapshot["geometry"]["vector_path"]["commands"]
    assert len(commands) == 3
    expected_points = [axes.c2p(value, value) for value in (-1.0, 0.0, 1.0)]
    for command, expected in zip(commands, expected_points):
        payload = command.get("move_to") or command.get("line_to")
        assert payload is not None, command
        actual = payload["to"]
        assert_close(actual["x"], expected.x)
        assert_close(actual["y"], expected.y)


class RetainedAxesPlot(Scene):
    def construct(self):
        axes = Axes(
            x_range=[-10, 10.3, 1],
            y_range=[-1.5, 1.5, 1],
            x_length=10,
            y_length=6,
            axis_config={"color": GREEN},
            x_axis_config={
                "numbers_with_elongated_ticks": [-10, -8, -6, -4, -2, 0, 2, 4, 6, 8, 10],
            },
            tips=False,
        )
        assert isinstance(axes, VGroup)
        assert len(axes.x_axis.ticks) == 20
        assert len(axes.y_axis.ticks) == 2
        assert len(axes.get_axes()) == 2
        assert issubclass(FunctionGraph, ParametricFunction)

        point = axes.c2p(3.25, -0.75)
        recovered = axes.p2c(point)
        assert_close(recovered[0], 3.25)
        assert_close(recovered[1], -0.75)

        probe_calls = []
        axes.plot(
            lambda x: (probe_calls.append(x), x * x)[1],
            x_range=[-1, 1, 1],
            use_smoothing=False,
        )
        assert probe_calls == [-1.0, 0.0, 1.0]

        copied = axes.copy()
        assert isinstance(copied, Axes)
        assert copied is not axes
        assert copied.x_axis is not axes.x_axis
        assert copied.y_axis is not axes.y_axis
        original_origin = axes.get_origin()
        assert_point_close(copied.get_origin(), original_origin)
        copied.to_corner(UL)
        assert (copied.get_origin() - original_origin).length() > 0.25
        assert_point_close(axes.get_origin(), original_origin)
        copied_point = copied.c2p(3.25, -0.75)
        copied_coords = copied.p2c(copied_point)
        assert_close(copied_coords[0], 3.25)
        assert_close(copied_coords[1], -0.75)
        copied_identity = copied.plot(
            lambda x: x,
            x_range=[-1, 1, 1],
            use_smoothing=False,
        )
        assert_three_point_graph_matches_axes(copied, copied_identity)

        origin_before = axes.get_origin()
        axes.shift(RIGHT * 1.25 + UP * 0.5)
        axes.scale(0.8)
        axes.rotate(0.35)
        origin_after = axes.get_origin()
        assert (origin_after - origin_before).length() > 0.25

        transformed_point = axes.c2p(3.25, -0.75)
        transformed_coords = axes.p2c(transformed_point)
        assert_close(transformed_coords[0], 3.25)
        assert_close(transformed_coords[1], -0.75)

        identity_graph = axes.plot(
            lambda x: x,
            x_range=[-1, 1, 1],
            use_smoothing=False,
            color=YELLOW,
        )
        assert isinstance(identity_graph, ParametricFunction)
        assert_three_point_graph_matches_axes(axes, identity_graph)

        sin_graph = axes.plot(lambda x: math.sin(x), color=BLUE)
        cos_graph = axes.plot(lambda x: math.cos(x), color=RED)
        assert isinstance(sin_graph, ParametricFunction)
        assert isinstance(cos_graph, ParametricFunction)
        self.add(axes, sin_graph, cos_graph)
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
    axesSource,
  );
  assert.equal(result.kind, "scene_document");
  assert.equal(result.document.objects.length, 26);
  assert.equal(result.document.tracks.length, 0);

  const geometryKinds = result.document.objects.map(
    (object) => Object.keys(object.geometry)[0],
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "line").length,
    24,
    "Axes must flatten to retained line/tick leaves",
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "vector_path").length,
    2,
    "Axes.plot must lower to ordinary retained VectorPath curves",
  );
  assert.deepEqual(errors, [], `browser errors while testing retained Axes:\n${errors.join("\n")}`);
  console.log(
    "Retained Axes smoke passed: VGroup type, copy/placement independence, shared line/tick families, transform-safe c2p/p2c, exact transformed plot points, and ParametricFunction results.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
