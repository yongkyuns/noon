import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";
import { rasterFixtureSource } from "./manim-raster-support.mjs";

const plotExampleImport = "from manim import *";
const upstreamPlotExample = await readFile(
  new URL("../parity/manim-v0.21/upstream-examples/plot_example.py", import.meta.url),
  "utf8",
);
assert.equal(
  upstreamPlotExample.split(plotExampleImport).length - 1,
  1,
  "pinned PlotExample must contain exactly one Manim wildcard import",
);
assert.ok(
  upstreamPlotExample.startsWith(`${plotExampleImport}\n`),
  "pinned PlotExample must preserve the upstream import as its first line",
);
const adaptedPlotExample = upstreamPlotExample.replace(plotExampleImport, "from noon import *");
assert.equal(
  adaptedPlotExample.replace("from noon import *", plotExampleImport),
  upstreamPlotExample,
  "PlotExample execution may change only the import line",
);
const plotExampleSource = rasterFixtureSource(adaptedPlotExample, "PlotExample");

async function qualifyPlotExampleRasterHost(context, baseUrl, expectedBackend) {
  const page = await context.newPage();
  try {
    page.setDefaultTimeout(90_000);
    await page.goto(`${baseUrl}/web/manim-raster-host.html`);
    await page.waitForFunction(() => window.noonHostRaster);
    const loaded = await page.evaluate(
      source => window.noonHostRaster.load(source, 1),
      plotExampleSource,
    );
    assert.equal(loaded.kind, "semantic_execution");
    assert.equal(loaded.duration, 0);
    assert.equal(loaded.objectCount, 57);
    assert.equal(loaded.rendererBackend, expectedBackend);
    const metrics = await page.evaluate(
      () => window.noonHostRaster.renderThrough(0, [0]),
    );
    assert.equal(metrics.error, null);
    assert.equal(metrics.presented, true);
    assert.equal(metrics.time, 0);
    assert.equal(metrics.objectCount, 57);
    const frame = await page.evaluate(() => window.noonHostRaster.debugFrame());
    assert.equal(frame.time, 0);
    assert.equal(frame.objects.length, 57);
    assert.ok(frame.objects.every(object => object.present));
    console.log(`[PASS] ${expectedBackend}: exact upstream PlotExample raster-host frame`);
  } finally {
    await page.close();
  }
}

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_PLOTTING_ARTIFACTS ?? "plotting-artifacts",
  cases: [
    { id: "number-line-defaults", sourcePath: "parity/manim-v0.21/core-examples/number_line_defaults.py", scene: "NumberLineDefaults", factory: "createNumberLineDefaultsRenderer", objectCount: 32, duration: 0.2, sampleTime: 0.2, playback: "live" },
    { id: "coordinates", file: "coordinate_plotting_static.py", factory: "createCoordinatePlottingRenderer", objectCount: 17 },
    { id: "graph-queries", file: "graph_queries.py", factory: "createGraphQueriesRenderer", objectCount: 15 },
    { id: "polar-plane", file: "polar_plane.py", factory: "createPolarPlaneRenderer", objectCount: 34 },
    { id: "number-plane", file: "number_plane.py", factory: "createNumberPlaneRenderer", objectCount: 26 },
    { id: "complex-plane", file: "complex_plane.py", factory: "createNumberPlaneRenderer", objectCount: 24 },
    { id: "area", file: "area_helpers.py", factory: "createAreaHelpersRenderer", objectCount: 26 },
    { id: "implicit", file: "implicit_plotting.py", factory: "createImplicitPlottingRenderer", objectCount: 16 },
  ],
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
    const logLabelsSource = `from noon import *

class LogBaseNumberLineLabels(Scene):
    async def construct(self):
        await prepare_latex()
        linear = NumberLine((-1, 1, 1), include_numbers=True)
        assert tuple(label.get_value() for label in linear.numbers) == (-1, 0, 1)
        line = NumberLine(
            (-2, 2, 1), scaling=LogBase(base=10), include_numbers=True,
            decimal_number_config={"num_decimal_places": 0},
        )
        assert tuple(label._source for label in line.numbers) == (
            "10^{-2}", "10^{-1}", "10^{0}", "10^{1}", "10^{2}",
        )
        assert all(isinstance(label, Integer) for label in line.numbers)
        assert tuple(label.get_value() for label in line.numbers) == (10, 10, 10, 10, 10)
        for label, coordinate in zip(line.numbers, (0.01, 0.1, 1, 10, 100)):
            assert abs(label.get_center().x - line.number_to_point(coordinate).x) < 1e-6
        assert line.labels is line.numbers
        self.add(line, linear)
        await self.wait(0.05)
`;
    const logLabels = await qualifyPythonPlayback(
      context, baseUrl, logLabelsSource, [0, 0.05],
    );
    assert.equal(logLabels.rendererBackend, expectedBackend);
    assert.equal(logLabels.presented, true);
    assert.ok(Math.abs(logLabels.authoredDuration - 0.05) < 1e-6);
    console.log(`[PASS] ${expectedBackend}: LogBase NumberLine exponent labels`);
    const page = await context.newPage();
    try {
      page.setDefaultTimeout(90_000);
      await page.goto(`${baseUrl}/web/manim-compat-smoke.html`);
      await page.waitForFunction(() => window.noonManimCompat);
      const lifecycle = await page.evaluate(async () => (await window.noonManimCompat.ready()).plotting);
      assert.equal(lifecycle.backend, expectedBackend);
      assert.equal(lifecycle.objectCount, 14);
      assert.ok(Math.abs(lifecycle.duration - 0.8) < 1e-6);
      assert.ok(lifecycle.presentedFrames > 0);
      const plotExample = await page.evaluate(
        source => window.noonManimCompat.runLive(source),
        plotExampleSource,
      );
      assert.equal(plotExample.duration, 0, "upstream PlotExample is static");
      assert.equal(plotExample.frame.time, 0, "static PlotExample is observed at authored time zero");
      assert.equal(plotExample.metrics.backend, expectedBackend);
      assert.equal(plotExample.metrics.objectCount, 57,
        "three copied Axes should retain 54 line/tick leaves and three curves");
      assert.ok(plotExample.metrics.drawCalls > 0);
      assert.ok(plotExample.metrics.presentedFrames > 0);
      assert.equal(plotExample.frame.objects.length, 57);
      assert.ok(plotExample.frame.objects.every(object => object.present),
        "all exact-source PlotExample objects must be present in the retained frame");
      const strokeCounts = new Map();
      for (const object of plotExample.frame.objects) {
        const { red, green, blue } = object.stroke;
        const color = [red, green, blue].map(channel => Number(channel.toFixed(6))).join(",");
        strokeCounts.set(color, (strokeCounts.get(color) ?? 0) + 1);
      }
      assert.equal(strokeCounts.get("1,1,1"), 54,
        "copied Axes must remain ordinary white line/tick objects");
      const curves = [...strokeCounts].filter(([color]) => color !== "1,1,1");
      assert.equal(curves.length, 3, "the three upstream curve colors must remain distinct");
      assert.deepEqual(curves.map(([, count]) => count).sort(), [1, 1, 1]);
      const curveRows = plotExample.frame.objects.filter(object => {
        const { red, green, blue } = object.stroke;
        return [red, green, blue].some(channel => Math.abs(channel - 1) > 1e-6)
          || Math.abs(red - green) > 1e-6 || Math.abs(green - blue) > 1e-6;
      });
      assert.equal(curveRows.length, 3);
      assert.ok(curveRows.every(object => object.bounds.width > 0 && object.bounds.height > 0),
        "each upstream log curve must retain two-dimensional sampled bounds");
      console.log(`[PASS] ${expectedBackend}: exact upstream PlotExample Python-host frame`);
      await qualifyPlotExampleRasterHost(context, baseUrl, expectedBackend);
      const labelSource = `from noon import *

class NumberPlaneNumericLabels(Scene):
    async def construct(self):
        plane = NumberPlane(
            x_range=(-2, 2, 1), y_range=(-1, 1, 0.5),
            axis_config={"include_numbers": True, "numbers_to_exclude": [0],
                         "font_size": 20, "line_to_number_buff": 0.12},
            x_axis_config={"numbers_to_include": [-1, 1]},
            y_axis_config={"include_numbers": False,
                           "numbers_to_include": [-0.5, 0.5],
                           "decimal_number_config": {"num_decimal_places": 1}},
        )
        assert len(plane.x_axis.numbers.submobjects) == 2
        assert len(plane.y_axis.numbers.submobjects) == 2
        assert tuple(label.source for label in plane.x_axis.numbers) == ("-1", "1")
        assert plane.x_axis.number_label_decimal_places == 0
        assert plane.y_axis.number_label_decimal_places == 1
        self.add(plane)
        await self.wait(0.05)
`;
      const labels = await qualifyPythonPlayback(context, baseUrl, labelSource, [0, 0.05]);
      assert.equal(labels.rendererBackend, expectedBackend);
      assert.equal(labels.presented, true);
      assert.ok(labels.objectCount > 0);
      assert.ok(Math.abs(labels.authoredDuration - 0.05) < 1e-6);
      console.log(`[PASS] ${expectedBackend}: NumberPlane native numeric labels`);
      const accessorSource = `from noon import *

class AxesAccessors(Scene):
    async def construct(self):
        axes = Axes((-2, 2, 1), (-1, 1, 0.5),
                    x_length=4, y_length=2, tips=False)
        assert axes.get_axes()[0] is axes.x_axis
        assert axes.get_axis(1) is axes.y_axis
        assert axes.get_x_axis() is axes.x_axis
        assert axes.get_y_axis() is axes.y_axis
        assert abs(axes.get_origin().x - axes.coords_to_point(0, 0).x) < 1e-6
        assert abs(axes.get_x_unit_size() - axes.x_axis.get_unit_size()) < 1e-6
        assert abs(axes.get_y_unit_size() - axes.y_axis.get_unit_size()) < 1e-6
        self.add(axes)
        await self.wait(0.05)
`;
      const accessors = await qualifyPythonPlayback(context, baseUrl, accessorSource, [0, 0.05]);
      assert.equal(accessors.rendererBackend, expectedBackend);
      assert.equal(accessors.presented, true);
      assert.ok(accessors.objectCount > 0);
      assert.ok(Math.abs(accessors.authoredDuration - 0.05) < 1e-6);
      console.log(`[PASS] ${expectedBackend}: Axes coordinate accessors`);
      const addCoordinatesSource = `from noon import *

class AxesAddCoordinates(Scene):
    async def construct(self):
        await prepare_latex()
        default_axes = Axes((-2, 2, 1), (-1, 1, 1), tips=False)
        assert default_axes.add_coordinates() is default_axes
        assert len(default_axes.coordinate_labels.submobjects) == 2
        assert default_axes.x_axis.numbers.submobjects
        assert default_axes.y_axis.numbers.submobjects
        assert all(isinstance(label, DecimalNumber)
                   for label in default_axes.x_axis.numbers)
        assert not any(label.get_value() == 0 for label in default_axes.x_axis.numbers)
        assert not any(label.get_value() == 0 for label in default_axes.y_axis.numbers)

        configured = Axes(
            (0, 2, 1), (-1, 1, 1), tips=False,
            x_axis_config={"numbers_to_exclude": (0, 2)},
            y_axis_config={"numbers_to_exclude": (-1,)},
        )
        configured.add_coordinates((0, 1, 2), (-1, 0, 1))
        assert tuple(label.get_value() for label in configured.x_axis.numbers) == (1,)
        assert tuple(label.get_value() for label in configured.y_axis.numbers) == (0, 1)

        override = Axes((0, 2, 1), (-1, 1, 1), tips=False)
        override.add_coordinates((0, 1, 2), (-1, 0, 1), excluding=(2,))
        assert tuple(label.get_value() for label in override.x_axis.numbers) == (0, 1)
        assert tuple(label.get_value() for label in override.y_axis.numbers) == (-1, 0, 1)

        x_only = Axes((-2, 2, 1), (-1, 1, 1), tips=False)
        x_only.add_coordinates((-1, 1))
        assert len(x_only.coordinate_labels.submobjects) == 1
        assert all(isinstance(label, DecimalNumber) for label in x_only.x_axis.numbers)
        assert tuple(label.source for label in x_only.x_axis.numbers) == ("-1", "1")
        assert not hasattr(x_only.y_axis, "numbers")

        x_default = Axes((-2, 2, 1), (-1, 1, 1), tips=False)
        x_default.add_coordinates(None)
        assert len(x_default.coordinate_labels.submobjects) == 1
        assert x_default.x_axis.numbers.submobjects
        assert not hasattr(x_default.y_axis, "numbers")

        custom = Axes((-2, 2, 1), (-1, 1, 1), tips=False)
        custom.add_coordinates({-1: "Left", 1: "Right"})
        assert len(custom.coordinate_labels.submobjects) == 1
        assert tuple(label.source for label in custom.x_axis.labels) == ("Left", "Right")

        mixed = Axes((-2, 2, 1), (-1, 1, 1), tips=False)
        mixed.add_coordinates({0: "origin"}, (-1, 1))
        assert len(mixed.coordinate_labels.submobjects) == 2
        assert tuple(label.source for label in mixed.x_axis.labels) == ("origin",)
        assert tuple(label.source for label in mixed.y_axis.numbers) == ("-1", "1")

        self.add(default_axes, configured, override, x_only, x_default, custom, mixed)
        await self.wait(0.05)
`;
      const addCoordinates = await qualifyPythonPlayback(
        context, baseUrl, addCoordinatesSource, [0, 0.05],
      );
      assert.equal(addCoordinates.rendererBackend, expectedBackend);
      assert.equal(addCoordinates.presented, true);
      assert.ok(addCoordinates.objectCount > 0);
      assert.ok(Math.abs(addCoordinates.authoredDuration - 0.05) < 1e-6);
      console.log(`[PASS] ${expectedBackend}: Axes.add_coordinates positional/default/custom labels`);
      const numberPlaneCoordinatesSource = `from noon import *

class NumberPlaneAddCoordinates(Scene):
    async def construct(self):
        await prepare_latex()
        default_plane = NumberPlane((-2, 2, 1), (-1, 1, 1))
        assert default_plane.add_coordinates() is default_plane
        assert len(default_plane.coordinate_labels.submobjects) == 2
        assert default_plane.x_axis.numbers.submobjects
        assert default_plane.y_axis.numbers.submobjects

        x_only = NumberPlane((-2, 2, 1), (-1, 1, 1))
        x_only.add_coordinates((-1, 1))
        assert len(x_only.coordinate_labels.submobjects) == 1
        assert tuple(label.source for label in x_only.x_axis.numbers) == ("-1", "1")
        assert not hasattr(x_only.y_axis, "numbers")

        x_default = NumberPlane((-2, 2, 1), (-1, 1, 1))
        x_default.add_coordinates(None)
        assert len(x_default.coordinate_labels.submobjects) == 1
        assert x_default.x_axis.numbers.submobjects
        assert not hasattr(x_default.y_axis, "numbers")

        custom = NumberPlane((-2, 2, 1), (-1, 1, 1))
        custom.add_coordinates({-1: "Left", 1: "Right"})
        assert len(custom.coordinate_labels.submobjects) == 1
        assert tuple(label.source for label in custom.x_axis.labels) == ("Left", "Right")

        mixed = NumberPlane((-2, 2, 1), (-1, 1, 1))
        mixed.add_coordinates({0: "origin"}, (-1, 1))
        assert len(mixed.coordinate_labels.submobjects) == 2
        assert tuple(label.source for label in mixed.x_axis.labels) == ("origin",)
        assert tuple(label.source for label in mixed.y_axis.numbers) == ("-1", "1")

        self.add(default_plane, x_only, x_default, custom, mixed)
        await self.wait(0.05)
`;
      const numberPlaneCoordinates = await qualifyPythonPlayback(
        context, baseUrl, numberPlaneCoordinatesSource, [0, 0.05],
      );
      assert.equal(numberPlaneCoordinates.rendererBackend, expectedBackend);
      assert.equal(numberPlaneCoordinates.presented, true);
      assert.ok(numberPlaneCoordinates.objectCount > 0);
      assert.ok(Math.abs(numberPlaneCoordinates.authoredDuration - 0.05) < 1e-6);
      console.log(`[PASS] ${expectedBackend}: NumberPlane.add_coordinates positional/default/custom labels`);
      const numberPlaneTicksSource = `from noon import *

class NumberPlaneAxisTicks(Scene):
    async def construct(self):
        plane = NumberPlane(
            (-2, 2, 1), (-1, 1, 1),
            axis_config={"include_ticks": True, "tick_size": 0.2,
                         "exclude_origin_tick": True},
            y_axis_config={"exclude_origin_tick": False, "tick_size": 0.3},
        )
        assert len(plane.x_axis.ticks.submobjects) == 4
        assert len(plane.y_axis.ticks.submobjects) == 2
        assert plane.x_axis.include_ticks is True
        assert plane.y_axis.include_ticks is True
        assert plane.x_axis.exclude_origin_tick is True
        assert plane.y_axis.exclude_origin_tick is True
        assert plane.x_axis.tick_size == 0.2
        assert plane.y_axis.tick_size == 0.3
        assert tuple(round(tick.get_center().x) for tick in plane.x_axis.ticks) == (-2, -1, 1, 2)
        assert tuple(round(tick.get_center().y) for tick in plane.y_axis.ticks) == (-1, 1)
        self.add(plane)
        await self.wait(0.05)
`;
      const numberPlaneTicks = await qualifyPythonPlayback(
        context, baseUrl, numberPlaneTicksSource, [0, 0.05],
      );
      assert.equal(numberPlaneTicks.rendererBackend, expectedBackend);
      assert.equal(numberPlaneTicks.presented, true);
      assert.ok(numberPlaneTicks.objectCount > 0);
      assert.ok(Math.abs(numberPlaneTicks.authoredDuration - 0.05) < 1e-6);
      console.log(`[PASS] ${expectedBackend}: NumberPlane per-axis ticks and Axes origin exclusion`);
      const numberPlaneStylesSource = `from noon import *

class NumberPlaneAxisStyles(Scene):
    async def construct(self):
        plane = NumberPlane(
            (-2, 2, 1), (-1, 1, 1),
            axis_config={"stroke_width": 2, "stroke_opacity": 0.8, "opacity": 0.9},
            x_axis_config={
                "color": GREEN, "stroke_width": 4,
                "stroke_opacity": 0.4, "opacity": 0.6,
            },
            y_axis_config={"stroke_color": BLUE},
        )
        x_shaft, y_shaft = plane.x_axis.shaft, plane.y_axis.shaft
        def color_matches(actual, expected):
            return all(abs(a - e) < 1e-6 for a, e in zip(
                (actual.red, actual.green, actual.blue),
                (expected.red, expected.green, expected.blue),
            ))
        assert color_matches(x_shaft.get_stroke_color(), GREEN)
        assert color_matches(y_shaft.get_stroke_color(), BLUE)
        assert abs(x_shaft.get_stroke_width() - 4) < 1e-6
        assert abs(y_shaft.get_stroke_width() - 2) < 1e-6
        assert abs(x_shaft.get_stroke_opacity() - 0.4) < 1e-6
        assert abs(y_shaft.get_stroke_opacity() - 0.8) < 1e-6
        self.add(plane)
        await self.wait(0.05)
`;
      const numberPlaneStyles = await qualifyPythonPlayback(
        context, baseUrl, numberPlaneStylesSource, [0, 0.05],
      );
      assert.equal(numberPlaneStyles.rendererBackend, expectedBackend);
      assert.equal(numberPlaneStyles.presented, true);
      assert.ok(numberPlaneStyles.objectCount > 0);
      assert.ok(Math.abs(numberPlaneStyles.authoredDuration - 0.05) < 1e-6);
      console.log(`[PASS] ${expectedBackend}: NumberPlane independent axis stroke styles`);
      const gridLinesSource = `from noon import *

class NumberPlaneGridLineAliases(Scene):
    async def construct(self):
        plane = NumberPlane((-2, 2, 1), (-1, 1, 1), faded_line_ratio=1)
        assert plane.x_lines.submobjects
        assert plane.y_lines.submobjects
        assert all(any(line is member for member in plane.background_lines.submobjects)
                   for line in plane.x_lines.submobjects)
        assert all(any(line is member for member in plane.background_lines.submobjects)
                   for line in plane.y_lines.submobjects)
        def parallel_to(line, axis):
            start, end = line.get_start(), line.get_end()
            axis_start, axis_end = axis.shaft.get_start(), axis.shaft.get_end()
            dx, dy = end.x - start.x, end.y - start.y
            ax, ay = axis_end.x - axis_start.x, axis_end.y - axis_start.y
            return abs(ax * dy - ay * dx) <= 1e-6 * ((ax * ax + ay * ay) ** 0.5) * ((dx * dx + dy * dy) ** 0.5)
        assert all(parallel_to(line, plane.x_axis) for line in plane.x_lines.submobjects)
        assert all(parallel_to(line, plane.y_axis) for line in plane.y_lines.submobjects)
        self.add(plane)
        await self.wait(0.05)
`;
      const gridLines = await qualifyPythonPlayback(context, baseUrl, gridLinesSource, [0, 0.05]);
      assert.equal(gridLines.rendererBackend, expectedBackend);
      assert.equal(gridLines.presented, true);
      assert.ok(gridLines.objectCount > 0);
      assert.ok(Math.abs(gridLines.authoredDuration - 0.05) < 1e-6);
      console.log(`[PASS] ${expectedBackend}: NumberPlane x_lines/y_lines aliases`);
      const axisLabelsSource = `from noon import *

class AxesLabels(Scene):
    async def construct(self):
        await prepare_latex()
        axes = Axes((-2, 2, 1), (-1, 1, 0.5), x_length=4, y_length=2, tips=False)
        explicit_x = axes.get_x_axis_label(Text("x-values"), edge=DOWN,
                                          direction=DOWN, buff=0.2)
        explicit_y = axes.get_y_axis_label("y", edge=LEFT,
                                          direction=LEFT, buff=0.2)
        labels = axes.get_axis_labels("x", Text("y-values"))
        assert axes.axis_labels is labels
        assert len(labels.submobjects) == 2
        assert all(getattr(label, "_semantic_family_handle", None) is not None
                   or getattr(label, "_semantic_handle", None) is not None
                   for label in labels.submobjects)

        plane = NumberPlane((-2, 2, 1), (-1, 1, 1), faded_line_ratio=1)
        plane_labels = plane.get_axis_labels(Text("u"), "v")
        assert plane.axis_labels is plane_labels
        assert len(plane_labels.submobjects) == 2

        edge_axes = Axes((-2, 2, 1), (-1, 1, 1), tips=False)
        edge_axes.shift(RIGHT * 6.5 + UP * 3.5)
        edge_label = edge_axes.get_x_axis_label("edge", edge=UR, direction=UR, buff=0.1)
        assert edge_label.get_right().x <= DEFAULT_FRAME_WIDTH / 2 - MED_SMALL_BUFF + 1e-6
        assert edge_label.get_top().y <= DEFAULT_FRAME_HEIGHT / 2 - MED_SMALL_BUFF + 1e-6

        self.add(axes, explicit_x, explicit_y, labels, plane, plane_labels,
                 edge_axes, edge_label)
        await self.wait(0.05)
`;
      const axisLabels = await qualifyPythonPlayback(context, baseUrl, axisLabelsSource, [0, 0.05]);
      assert.equal(axisLabels.rendererBackend, expectedBackend);
      assert.equal(axisLabels.presented, true);
      assert.ok(axisLabels.objectCount > 0);
      assert.ok(Math.abs(axisLabels.authoredDuration - 0.05) < 1e-6);
      console.log(`[PASS] ${expectedBackend}: Axes and NumberPlane text labels`);
      const guideLinesSource = `from noon import *

class AxesGuideLines(Scene):
    async def construct(self):
        axes = Axes((-2, 2, 1), (-1, 1, 0.5), x_length=4, y_length=2, tips=False)
        axes.shift(RIGHT * 0.25)
        point = axes.c2p(1, 0.5)
        vertical = axes.get_vertical_line(point, line_func=Line, color=RED)
        horizontal = axes.get_horizontal_line(point, line_func=Line, color=BLUE)
        guides = axes.get_lines_to_point(point, line_func=Line)
        expected_vertical = axes.c2p(1, 0)
        expected_horizontal = axes.c2p(0, 0.5)
        def near(actual, expected):
            return abs(actual.x - expected.x) < 1e-6 and abs(actual.y - expected.y) < 1e-6
        assert near(vertical.get_start(), expected_vertical)
        assert near(horizontal.get_start(), expected_horizontal)
        assert len(guides.submobjects) == 2
        assert near(guides.submobjects[0].get_start(), expected_horizontal)
        assert near(guides.submobjects[1].get_start(), expected_vertical)
        self.add(axes, vertical, horizontal, guides)
        await self.wait(0.05)
`;
      const guideLines = await qualifyPythonPlayback(context, baseUrl, guideLinesSource, [0, 0.05]);
      assert.equal(guideLines.rendererBackend, expectedBackend);
      assert.equal(guideLines.presented, true);
      assert.ok(guideLines.objectCount > 0);
      assert.ok(Math.abs(guideLines.authoredDuration - 0.05) < 1e-6);
      console.log(`[PASS] ${expectedBackend}: Axes line-to-point guides`);
      const lineObservationsSource = `from noon import *

class NumberLineLineObservations(Scene):
    async def construct(self):
        line = NumberLine((-2, 2, 1), length=4, rotation=PI / 2,
                          scaling=LogBase(base=10), include_ticks=False)
        line.shift(RIGHT * 0.5)
        start, end = line.get_start_and_end()
        vector = line.get_vector()
        assert abs(start.x - 0.5) < 1e-6 and abs(start.y + 2.0) < 1e-6
        assert abs(end.x - 0.5) < 1e-6 and abs(end.y - 2.0) < 1e-6
        assert abs(vector.x) < 1e-6 and abs(vector.y - 4.0) < 1e-6
        assert abs(line.get_length() - 4.0) < 1e-6
        assert abs(line.get_angle() - PI / 2) < 1e-6
        projected = line.get_projection((3.5, 1.0))
        assert abs(projected.x - 0.5) < 1e-6 and abs(projected.y - 1.0) < 1e-6
        line.set_length(6)
        assert abs(line.get_length() - 6.0) < 1e-6
        line.set_angle(PI / 4)
        assert abs(line.get_angle() - PI / 4) < 1e-6
        assert abs(line.get_slope() - 1.0) < 1e-6
        line.set_angle(0, about_point=ORIGIN)
        assert abs(line.get_angle()) < 1e-6
        assert abs(line.get_length() - 6.0) < 1e-6

        axes = Axes((-2, 2, 1), (-2, 2, 1), x_length=4, y_length=4,
                    x_axis_config={"scaling": LogBase(base=10)}, tips=False)
        axes.shift(UP * 0.25)
        point = axes.c2p(1, 1)
        guide = axes.get_vertical_line(point, line_func=Line)
        expected = axes.c2p(1, 0)
        actual = guide.get_start()
        assert abs(actual.x - expected.x) < 1e-6
        assert abs(actual.y - expected.y) < 1e-6
        self.add(line, axes, guide)
        await self.wait(0.05)
`;
      const observations = await qualifyPythonPlayback(
        context, baseUrl, lineObservationsSource, [0, 0.05],
      );
      assert.equal(observations.rendererBackend, expectedBackend);
      assert.equal(observations.presented, true);
      assert.ok(observations.objectCount > 0);
      assert.ok(Math.abs(observations.authoredDuration - 0.05) < 1e-6);
      console.log(`[PASS] ${expectedBackend}: NumberLine line observations`);
      const numberLabelsSource = `from noon import *

class NumberLineDecimalNumberLabels(Scene):
    async def construct(self):
        await prepare_latex()
        line = NumberLine((-2, 2, 1), length=4, include_ticks=True)
        detached = line.get_number_mobjects(1, -1, 1)
        assert all(isinstance(label, DecimalNumber) for label in detached)
        assert tuple(label.get_value() for label in detached) == (1, -1, 1)
        assert len(line.submobjects) == 2
        assert line.add_numbers((0, 1, -1), excluding=(0,)) is line
        assert tuple(label.get_value() for label in line.numbers) == (1, -1)
        assert all(isinstance(label, DecimalNumber) for label in line.numbers)
        self.add(line, detached)
        await self.wait(0.05)
`;
      const numberLabels = await qualifyPythonPlayback(
        context, baseUrl, numberLabelsSource, [0, 0.05],
      );
      assert.equal(numberLabels.rendererBackend, expectedBackend);
      assert.equal(numberLabels.presented, true);
      assert.ok(numberLabels.objectCount > 0);
      assert.ok(Math.abs(numberLabels.authoredDuration - 0.05) < 1e-6);
      console.log(`[PASS] ${expectedBackend}: NumberLine DecimalNumber labels`);
      return lifecycle;
    } finally {
      await page.close();
    }
  },
});
