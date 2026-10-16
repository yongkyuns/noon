import assert from "node:assert/strict";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_PLOTTING_ARTIFACTS ?? "plotting-artifacts",
  cases: [
    { id: "number-line-defaults", sourcePath: "parity/manim-v0.21/core-examples/number_line_defaults.py", scene: "NumberLineDefaults", factory: "createNumberLineDefaultsRenderer", objectCount: 32, duration: 0.2, sampleTime: 0.2, playback: "live" },
    { id: "coordinates", file: "coordinate_plotting_static.py", factory: "createCoordinatePlottingRenderer", objectCount: 17 },
    { id: "graph-queries", file: "graph_queries.py", factory: "createGraphQueriesRenderer", objectCount: 15 },
    { id: "polar-plane", file: "polar_plane.py", factory: "createPolarPlaneRenderer", objectCount: 34 },
    { id: "number-plane", file: "number_plane.py", factory: "createNumberPlaneRenderer", objectCount: 24 },
    { id: "complex-plane", file: "complex_plane.py", factory: "createNumberPlaneRenderer", objectCount: 24 },
    { id: "area", file: "area_helpers.py", factory: "createAreaHelpersRenderer", objectCount: 26 },
    { id: "implicit", file: "implicit_plotting.py", factory: "createImplicitPlottingRenderer", objectCount: 16 },
  ],
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
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
        self.add(axes, explicit_x, explicit_y, labels, plane, plane_labels)
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
      return lifecycle;
    } finally {
      await page.close();
    }
  },
});
