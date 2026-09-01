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


def assert_line_graph_matches_axes(axes, graph, x_values, y_values):
    handle = _handles._handle_for(graph)
    assert handle is not None
    snapshot = json.loads(str(handle.snapshotJson()))
    commands = snapshot["geometry"]["vector_path"]["commands"]
    assert len(commands) == len(x_values)
    expected_points = [axes.c2p(x, y) for x, y in zip(x_values, y_values)]
    for command, expected in zip(commands, expected_points):
        payload = command.get("move_to") or command.get("line_to")
        assert payload is not None, command
        actual = payload["to"]
        assert_close(actual["x"], expected.x)
        assert_close(actual["y"], expected.y)
    return snapshot, expected_points


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

        graph_point = axes.i2gp(0.75, identity_graph)
        assert_point_close(graph_point, axes.c2p(0.75, 0.75))
        graph_coords = axes.i2gc(0.75, identity_graph)
        assert_close(graph_coords[0], 0.75)
        assert_close(graph_coords[1], 0.75)

        vertical_config = {}
        vertical = axes.get_vertical_line(
            graph_point,
            line_func=Line,
            line_config=vertical_config,
            color=PURE_GREEN,
            stroke_width=3,
        )
        assert_close(vertical_config["stroke_width"], 3)
        assert vertical_config["color"] == PURE_GREEN
        assert_point_close(vertical.start, axes.c2p(0.75, 0.0))
        assert_point_close(vertical.end, graph_point)

        horizontal = axes.get_horizontal_line(
            graph_point,
            line_func=Line,
            color=ORANGE,
        )
        assert_point_close(horizontal.start, axes.c2p(0.0, 0.75))
        assert_point_close(horizontal.end, graph_point)

        x_values = [0.0, 1.0, 1.0, 2.0]
        y_values = [0.5, 1.0, -0.5, 0.0]
        line_graph = axes.plot_line_graph(
            x_values=x_values,
            y_values=y_values,
            line_color=PURE_GREEN,
            stroke_width=4,
        )
        assert isinstance(line_graph, VDict)
        assert "line_graph" in line_graph
        assert "vertex_dots" in line_graph
        assert len(line_graph["vertex_dots"]) == 4
        line_snapshot, expected_vertices = assert_line_graph_matches_axes(
            axes, line_graph["line_graph"], x_values, y_values
        )
        assert_close(line_snapshot["style"]["stroke_width"], 0.04)
        for dot, expected in zip(line_graph["vertex_dots"], expected_vertices):
            assert_point_close(dot.get_center(), expected)

        lookup_line_graph = axes.plot_line_graph(
            x_values=[-1.0, 0.0, 1.0],
            y_values=[1.0, 0.0, 1.0],
            add_vertex_dots=False,
        )["line_graph"]
        lookup_point = axes.input_to_graph_point(0.5, lookup_line_graph)
        assert_point_close(lookup_point, axes.c2p(0.5, 0.5), tolerance=2e-4)
        assert_point_close(axes.i2gp(0.5, lookup_line_graph), lookup_point, tolerance=2e-4)
        try:
            axes.input_to_graph_point(3.0, lookup_line_graph)
            raise AssertionError("out-of-range generic graph lookup must fail")
        except ValueError as error:
            assert "x=3" in str(error)
            assert "not located in the range of the graph" in str(error)

        no_dots = axes.plot_line_graph(
            x_values=[0, 1],
            y_values=[0, 1],
            add_vertex_dots=False,
        )
        assert isinstance(no_dots, VDict)
        assert "line_graph" in no_dots
        assert "vertex_dots" not in no_dots

        try:
            axes.plot_line_graph([0, 1], [0])
            raise AssertionError("mismatched line-graph coordinates must fail")
        except ValueError:
            pass
        try:
            axes.plot_line_graph([0], [0], z_values=[1])
            raise AssertionError("nonzero z must fail in 2D Axes")
        except NotImplementedError:
            pass
        try:
            VDict(show_keys=True)
            raise AssertionError("VDict key labels must fail until exact Tex is available")
        except NotImplementedError:
            pass

        sin_graph = axes.plot(lambda x: math.sin(x), color=BLUE)
        cos_graph = axes.plot(lambda x: math.cos(x), color=RED)
        assert isinstance(sin_graph, ParametricFunction)
        assert isinstance(cos_graph, ParametricFunction)
        cos_point = axes.input_to_graph_point(0.25, cos_graph)
        assert_point_close(cos_point, axes.c2p(0.25, math.cos(0.25)))
        self.add(axes, sin_graph, cos_graph, vertical, horizontal, line_graph)
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
  assert.equal(result.document.objects.length, 33);
  assert.equal(result.document.tracks.length, 0);

  const geometryKinds = result.document.objects.map(
    (object) => Object.keys(object.geometry)[0],
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "line").length,
    26,
    "Axes and projection helpers must flatten to ordinary retained line geometry",
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "vector_path").length,
    3,
    "Axes.plot and plot_line_graph must lower to ordinary retained VectorPath geometry",
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "circle").length,
    4,
    "plot_line_graph vertex dots must remain ordinary retained circle geometry",
  );
  assert.deepEqual(errors, [], `browser errors while testing retained Axes:\n${errors.join("\n")}`);
  console.log(
    "Retained Axes smoke passed: transformed c2p/p2c, authored and generic i2gp, retained projections, plot curves, and VDict line graphs with shared corner-path geometry and optional retained dots.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
