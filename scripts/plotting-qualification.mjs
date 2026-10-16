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
            x_range=(-2, 2, 0.5), y_range=(-1, 1, 0.5),
            axis_config={"include_numbers": True, "numbers_to_exclude": [0],
                         "font_size": 20, "line_to_number_buff": 0.12},
            x_axis_config={"numbers_to_include": [-1, 1]},
            y_axis_config={"include_numbers": False,
                           "numbers_to_include": [-0.5, 0.5],
                           "decimal_number_config": {"num_decimal_places": 1}},
        )
        assert len(plane.x_axis.numbers.submobjects) == 2
        assert len(plane.y_axis.numbers.submobjects) == 2
        assert plane.x_axis.number_label_decimal_places == 1
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
      return lifecycle;
    } finally {
      await page.close();
    }
  },
});
