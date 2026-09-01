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


def assert_corner_path(graph, expected_points):
    handle = _handles._handle_for(graph)
    assert handle is not None
    snapshot = json.loads(str(handle.snapshotJson()))
    commands = snapshot["geometry"]["vector_path"]["commands"]
    assert len(commands) == len(expected_points)
    for command, expected in zip(commands, expected_points):
        payload = command.get("move_to") or command.get("line_to")
        assert payload is not None, command
        actual = payload["to"]
        assert_close(actual["x"], expected[0])
        assert_close(actual["y"], expected[1])
    return snapshot


def snapshot_for(mobject):
    handle = _handles._handle_for(mobject)
    assert handle is not None
    return json.loads(str(handle.snapshotJson()))


def assert_stroke_color(mobject, expected):
    stroke = snapshot_for(mobject)["style"]["stroke"]
    assert_close(stroke["red"], expected.red)
    assert_close(stroke["green"], expected.green)
    assert_close(stroke["blue"], expected.blue)
    assert_close(stroke["alpha"], expected.alpha)


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
        assert issubclass(NumberPlane, Axes)

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

        cos_graph.set_color(MAROON)
        secant_x = 0.25
        secant_dx = 0.5
        secant_p1 = axes.i2gp(secant_x, cos_graph)
        secant_p2 = axes.i2gp(secant_x + secant_dx, cos_graph)
        secant_interim = Vec2(secant_p2.x, secant_p1.y)
        secant_group = axes.get_secant_slope_group(
            secant_x,
            cos_graph,
            dx=secant_dx,
            dx_line_color=PURE_GREEN,
            secant_line_color=PURPLE,
            secant_line_length=4.0,
        )
        assert isinstance(secant_group, VGroup)
        assert len(secant_group) == 3
        assert secant_group[0] is secant_group.dx_line
        assert secant_group[1] is secant_group.df_line
        assert secant_group[2] is secant_group.secant_line
        assert_point_close(secant_group.dx_line.start, secant_p1)
        assert_point_close(secant_group.dx_line.end, secant_interim)
        assert_point_close(secant_group.df_line.start, secant_interim)
        assert_point_close(secant_group.df_line.end, secant_p2)
        assert_stroke_color(secant_group.dx_line, PURE_GREEN)
        assert_stroke_color(secant_group.df_line, MAROON)
        assert_stroke_color(secant_group.secant_line, PURPLE)
        assert_point_close(
            secant_group.secant_line.get_center(),
            (secant_p1 + secant_p2) * 0.5,
        )
        raw_secant_length = (secant_p2 - secant_p1).length()
        secant_snapshot = snapshot_for(secant_group.secant_line)
        expected_secant_scale = 4.0 / raw_secant_length
        assert_close(secant_snapshot["transform"]["scale"]["x"], expected_secant_scale)
        assert_close(secant_snapshot["transform"]["scale"]["y"], expected_secant_scale)

        default_dx = (axes.x_range[1] - axes.x_range[0]) / 10.0
        default_secant = axes.get_secant_slope_group(
            secant_x,
            cos_graph,
            dx=0.0,
            include_secant_line=False,
        )
        assert len(default_secant) == 2
        assert not hasattr(default_secant, "secant_line")
        assert_point_close(
            default_secant.df_line.end,
            axes.i2gp(secant_x + default_dx, cos_graph),
        )
        assert_stroke_color(default_secant.dx_line, PURE_YELLOW)
        assert_stroke_color(default_secant.df_line, MAROON)

        negative_secant = axes.get_secant_slope_group(
            secant_x,
            cos_graph,
            dx=-0.25,
            include_secant_line=False,
        )
        assert len(negative_secant) == 2
        assert_point_close(
            negative_secant.df_line.end,
            axes.i2gp(secant_x - 0.25, cos_graph),
        )

        for label_kwargs in ({"dx_label": "dx"}, {"dy_label": "dy"}):
            try:
                axes.get_secant_slope_group(secant_x, cos_graph, **label_kwargs)
                raise AssertionError("secant labels must wait for exact retained text")
            except NotImplementedError as error:
                assert "MathTex/number labels" in str(error)

        parametric_calls = []

        def parametric_fn(t):
            parametric_calls.append(float(t))
            return [math.cos(t), math.sin(t), 42.0]

        parametric = axes.plot_parametric_curve(
            parametric_fn,
            t_range=[0.0, math.pi, math.pi / 2.0],
            use_smoothing=False,
            color=PURPLE,
        )
        assert isinstance(parametric, ParametricFunction)
        assert len(parametric_calls) == 3
        for actual, expected in zip(
            parametric_calls, [0.0, math.pi / 2.0, math.pi]
        ):
            assert_close(actual, expected)
        assert_close(parametric.t_min, 0.0)
        assert_close(parametric.t_max, math.pi)
        parametric_handle = _handles._handle_for(parametric)
        assert parametric_handle is not None
        parametric_snapshot = json.loads(str(parametric_handle.snapshotJson()))
        parametric_commands = parametric_snapshot["geometry"]["vector_path"]["commands"]
        assert len(parametric_commands) == 3
        parametric_expected = [
            axes.c2p(1.0, 0.0),
            axes.c2p(0.0, 1.0),
            axes.c2p(-1.0, 0.0),
        ]
        for command, expected in zip(parametric_commands, parametric_expected):
            payload = command.get("move_to") or command.get("line_to")
            assert payload is not None, command
            actual = payload["to"]
            assert_close(actual["x"], expected.x)
            assert_close(actual["y"], expected.y)
        parametric_calls.clear()
        assert_point_close(
            parametric.get_point_from_function(math.pi / 2.0),
            axes.c2p(0.0, 1.0),
        )
        assert len(parametric_calls) == 1
        try:
            axes.plot_parametric_curve(parametric_fn, use_vectorized=True)
            raise AssertionError("vectorized parametric callbacks must fail explicitly")
        except NotImplementedError:
            pass

        direct_calls = []

        def direct_fn(t):
            direct_calls.append(float(t))
            return [math.cos(t), math.sin(t), 0.0]

        direct_parametric = ParametricFunction(
            direct_fn,
            t_range=[0.0, math.pi, math.pi / 2.0],
            use_smoothing=False,
            color=TEAL,
        )
        assert isinstance(direct_parametric, VMobject)
        assert len(direct_calls) == 3
        for actual, expected in zip(direct_calls, [0.0, math.pi / 2.0, math.pi]):
            assert_close(actual, expected)
        direct_snapshot = assert_corner_path(
            direct_parametric,
            [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)],
        )
        assert_close(direct_parametric.t_min, 0.0)
        assert_close(direct_parametric.t_max, math.pi)
        assert_close(direct_parametric.t_step, math.pi / 2.0)
        expected_teal = [TEAL.red, TEAL.green, TEAL.blue, TEAL.alpha]
        stroke = direct_snapshot["style"]["stroke"]
        for channel, expected in zip(("red", "green", "blue", "alpha"), expected_teal):
            assert_close(stroke[channel], expected)
        direct_calls.clear()
        direct_point = direct_parametric.get_point_from_function(math.pi / 2.0)
        assert_close(direct_point[0], 0.0)
        assert_close(direct_point[1], 1.0)
        assert len(direct_calls) == 1

        function_graph = FunctionGraph(
            lambda x: x * x,
            x_range=[-1.0, 1.0, 1.0],
            color=MAROON,
            use_smoothing=False,
        )
        assert isinstance(function_graph, ParametricFunction)
        assert_corner_path(
            function_graph,
            [(-1.0, 1.0), (0.0, 0.0), (1.0, 1.0)],
        )
        assert_close(function_graph.get_function()(0.5), 0.25)
        function_point = function_graph.get_point_from_function(0.5)
        assert_close(function_point[0], 0.5)
        assert_close(function_point[1], 0.25)
        assert_close(function_point[2], 0.0)

        try:
            ParametricFunction(
                lambda t: [t, 0.0, 1.0],
                t_range=[0.0, 1.0, 1.0],
                use_smoothing=False,
            )
            raise AssertionError("direct nonzero-z parametric geometry must fail explicitly")
        except NotImplementedError:
            pass
        try:
            ParametricFunction(direct_fn, use_vectorized=True)
            raise AssertionError("direct vectorized callbacks must fail explicitly")
        except NotImplementedError:
            pass

        curve_1_calls = []
        curve_2_calls = []

        def curve_1_fn(x):
            curve_1_calls.append(float(x))
            return 4 * x - x ** 2

        def curve_2_fn(x):
            curve_2_calls.append(float(x))
            return 0.8 * x ** 2 - 3 * x + 4

        curve_1 = axes.plot(curve_1_fn, x_range=[0, 4], color=BLUE_C)
        curve_2 = axes.plot(curve_2_fn, x_range=[0, 4], color=GREEN_B)
        assert_close(curve_1.t_min, 0.0)
        assert_close(curve_1.t_max, 4.0)
        curve_1_calls.clear()
        curve_2_calls.clear()

        riemann_area = axes.get_riemann_rectangles(
            curve_1,
            x_range=[0.3, 0.6],
            dx=0.03,
            color=BLUE,
            fill_opacity=0.5,
        )
        assert isinstance(riemann_area, VGroup)
        assert len(riemann_area) == 10
        assert len(curve_1_calls) == 10
        for rectangle in riemann_area:
            handle = _handles._handle_for(rectangle)
            assert handle is not None
            snapshot = json.loads(str(handle.snapshotJson()))
            assert "rectangle" in snapshot["geometry"]
            assert_close(snapshot["style"]["fill"]["alpha"], 0.5)

        curve_1_calls.clear()
        curve_2_calls.clear()
        area = axes.get_area(
            curve_2,
            [2, 3],
            bounded_graph=curve_1,
            color=GREY,
            opacity=0.5,
        )
        assert curve_2_calls == [2.0, 3.0]
        assert curve_1_calls == [2.0, 3.0]
        area_handle = _handles._handle_for(area)
        assert area_handle is not None
        area_snapshot = json.loads(str(area_handle.snapshotJson()))
        assert "vector_path" in area_snapshot["geometry"]
        assert area_snapshot["geometry"]["vector_path"]["commands"][-1] == "close"

        try:
            axes.get_riemann_rectangles(curve_1, input_sample_type="other")
            raise AssertionError("invalid Riemann sample type must fail")
        except ValueError as error:
            assert str(error) == "Invalid input sample type"

        default_plane = NumberPlane()
        assert_close(default_plane.x_range[0], -DEFAULT_FRAME_WIDTH / 2.0)
        assert_close(default_plane.x_range[1], DEFAULT_FRAME_WIDTH / 2.0)
        assert_close(default_plane.y_range[0], -DEFAULT_FRAME_HEIGHT / 2.0)
        assert_close(default_plane.y_range[1], DEFAULT_FRAME_HEIGHT / 2.0)
        assert_close(default_plane.x_length, DEFAULT_FRAME_WIDTH)
        assert_close(default_plane.y_length, DEFAULT_FRAME_HEIGHT)

        plane = NumberPlane(
            x_range=[-2, 2, 1],
            y_range=[-2, 2, 1],
            x_length=8,
            y_length=12,
            faded_line_ratio=2,
            background_line_style={
                "stroke_color": RED,
                "stroke_width": 2,
                "stroke_opacity": 0.8,
            },
            faded_line_style={"stroke_opacity": 0.25},
        )
        assert isinstance(plane, Axes)
        assert len(plane) == 4
        assert plane[0] is plane.faded_lines
        assert plane[1] is plane.background_lines
        assert len(plane.x_lines) == 3
        assert len(plane.y_lines) == 3
        assert len(plane.faded_x_lines) == 4
        assert len(plane.faded_y_lines) == 4
        assert len(plane.background_lines) == 6
        assert len(plane.faded_lines) == 8
        assert plane.axis_config["include_ticks"] is False
        assert plane.x_axis_config["include_ticks"] is False
        assert plane.y_axis_config["label_direction"] == DR

        plane_origin = plane.get_origin()
        assert_point_close(plane.c2p(1.0, 1.0), plane_origin + Vec2(2.0, 3.0))

        background_handle = _handles._handle_for(plane.x_lines[0])
        faded_handle = _handles._handle_for(plane.faded_x_lines[0])
        assert background_handle is not None
        assert faded_handle is not None
        background_snapshot = json.loads(str(background_handle.snapshotJson()))
        faded_snapshot = json.loads(str(faded_handle.snapshotJson()))
        assert_close(background_snapshot["style"]["stroke_width"], 0.02)
        assert_close(background_snapshot["style"]["stroke"]["red"], RED.red)
        assert_close(background_snapshot["style"]["stroke"]["alpha"], 0.8)
        assert_close(faded_snapshot["style"]["stroke_width"], 0.04)
        assert_close(faded_snapshot["style"]["stroke"]["red"], WHITE.red)
        assert_close(faded_snapshot["style"]["stroke"]["green"], WHITE.green)
        assert_close(faded_snapshot["style"]["stroke"]["blue"], WHITE.blue)
        assert_close(faded_snapshot["style"]["stroke"]["alpha"], 0.25)

        plane.shift(LEFT * 0.75 + DOWN * 0.25)
        plane.scale(0.9)
        plane.rotate(-0.2)
        plane_point = plane.c2p(1.25, -0.75)
        plane_coords = plane.p2c(plane_point)
        assert_close(plane_coords[0], 1.25)
        assert_close(plane_coords[1], -0.75)

        copied_plane = plane.copy()
        assert isinstance(copied_plane, NumberPlane)
        assert copied_plane is not plane
        assert copied_plane.x_axis is not plane.x_axis
        assert copied_plane.x_lines[0] is not plane.x_lines[0]
        assert_point_close(copied_plane.get_origin(), plane.get_origin())
        copied_plane_point = copied_plane.c2p(1.25, -0.75)
        assert_point_close(copied_plane_point, plane_point)
        copied_plane.shift(RIGHT)
        assert (copied_plane.get_origin() - plane.get_origin()).length() > 0.5

        self.add(
            axes,
            sin_graph,
            cos_graph,
            secant_group,
            parametric,
            direct_parametric,
            function_graph,
            vertical,
            horizontal,
            line_graph,
            curve_1,
            curve_2,
            riemann_area,
            area,
            plane,
        )
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
  assert.equal(result.document.objects.length, 68);
  assert.equal(result.document.tracks.length, 0);

  const geometryKinds = result.document.objects.map(
    (object) => Object.keys(object.geometry)[0],
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "line").length,
    45,
    "Axes, NumberPlane, projection helpers, and secant groups must flatten to ordinary retained line geometry",
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "vector_path").length,
    9,
    "Axes/direct scalar and parametric plots, line graphs, and area polygons must remain ordinary retained VectorPath geometry",
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "circle").length,
    4,
    "plot_line_graph vertex dots must remain ordinary retained circle geometry",
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "rectangle").length,
    10,
    "Riemann rectangles must remain ordinary retained rectangle geometry",
  );
  assert.deepEqual(errors, [], `browser errors while testing retained Axes:\n${errors.join("\n")}`);
  console.log(
    "Retained Axes/NumberPlane smoke passed: transformed coordinates, retained grid families, direct/Axes parametric functions, FunctionGraph, authored/generic i2gp, secant slope groups, line graphs, two-phase Riemann rectangles, and bounded area polygons.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
