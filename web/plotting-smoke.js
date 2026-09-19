import { qualifyNumberLabels } from "./number-label-smoke.js";

// Executed by the existing Manim/Pyodide smoke gate, not a mocked Python host.
import { WasmAuthoringStore, WasmCoordinateOptions, WasmPlotSamplingPlan } from "./pkg/noon_web.js";

function near(actual, expected, label) {
  if (actual.length !== expected.length || actual.some((value, i) =>
      !Number.isFinite(value) || Math.abs(value - expected[i]) > 2e-5)) {
    throw new Error(`${label}: ${actual} != ${expected}`);
  }
}

function directWasmPreparation() {
  const store = new WasmAuthoringStore();
  const axes = store.createCoordinates(WasmCoordinateOptions.axes([2, 6, 1], [-6, -2, 1], 8, 4));
  const frame = axes.axesFrame();
  const plan = frame.plotPlan([2, 6, 1], [], undefined, undefined);
  const parameters = plan.parameters();
  near(Array.from(parameters), [2, 3, 4, 5, 6], "shared sample order");
  near(Array.from(frame.coordsToPoint(4, -4)), [0, 0], "positive/negative range midpoint");
  const curve = store.createManimGeometry(plan.functionSamples(parameters.map(() => -4), false));
  const path = curve.pathQuery();
  near(Array.from(path.start()), [-4, 0], "mapped native sample start");
  near(Array.from(path.end()), [4, 0], "mapped native sample end");
  let rejected = false;
  try {
    plan.functionSamples([1], false);
  } catch (error) {
    rejected = error.category === "invalid_input";
  }
  if (!rejected) throw new Error("sample-count mismatch must retain typed input rejection");
  path.free();
  curve.free();
  plan.free();
  frame.free();
  axes.free();
  store.free();

  // Adjacent large data values have no representable midpoint. The shared
  // mapper must still preserve the intended scene rectangle and curve bounds.
  const offsetStore = new WasmAuthoringStore();
  try {
    for (const range of [[1e16, 1e16 + 2, 2], [-1e16 - 2, -1e16, 2]]) {
      const options = WasmCoordinateOptions.axes(range, range, 8, 4);
      options.setTicks(false, 0.1, true);
      const offsetAxes = offsetStore.createCoordinates(options);
      const offsetFrame = offsetAxes.axesFrame();
      try {
        near(Array.from(offsetFrame.coordsToPoint(range[0], range[0])), [-4, -2], "large-offset start");
        near(Array.from(offsetFrame.coordsToPoint(range[1], range[1])), [4, 2], "large-offset end");
        const timed = offsetFrame.timeSeriesPlan([range[0], range[0], range[1], range[1]], 1);
        try {
          near(Array.from(timed.cursorPoints()), [-4, 0, 4, 0], "large-offset cursor centers");
        } finally { timed.free(); }
        const mapped = offsetStore.createManimGeometry(
          offsetFrame.sampledPlot([range[0], range[0], range[1], range[1]]));
        const mappedPath = mapped.pathQuery();
        try {
          near(Array.from(mappedPath.start()), [-4, -2], "large-offset data start");
          near(Array.from(mappedPath.end()), [4, 2], "large-offset data end");
        } finally { mappedPath.free(); mapped.free(); }
      } finally { offsetFrame.free(); offsetAxes.free(); }
    }
  } finally { offsetStore.free(); }

  // Exact parameter assertions matter here: a scene-space tolerance could
  // conceal a missing start on the tiny domains. Both points count to the cap.
  for (const range of [[0, 1e-300, 1e100], [0, Number.MIN_VALUE, 2], [1e308, 1.25e308, 1e308]]) {
    const boundary = WasmPlotSamplingPlan.parametric(range, [], undefined, 2);
    try {
      const values = Array.from(boundary.parameters());
      if (values.length !== 2 || values[0] !== range[0] || values[1] !== range[1]) {
        throw new Error(`extreme plot range lost its endpoints: ${values}`);
      }
    } finally { boundary.free(); }
    let limited = false;
    try {
      const unexpected = WasmPlotSamplingPlan.parametric(range, [], undefined, 1);
      unexpected.free();
    } catch (error) {
      limited = error.category === "resource_limit" && error.code === "plot.preparation";
    }
    if (!limited) throw new Error("two-endpoint plot did not retain exact budget rejection");
  }

  const split = WasmPlotSamplingPlan.parametric([-1, 1, 0.25], [0], 0.05, undefined);
  if (Array.from(split.parameters()).includes(0)) throw new Error("discontinuity gap was sampled");
  split.free();
}

const source = `
from noon import *
from math import sin, cos, pi


def near(actual, expected):
    assert len(actual) == len(expected)
    assert all(abs(a - b) < 2e-5 for a, b in zip(actual, expected)), (actual, expected)


class PlottingQualification(Scene):
    def construct(self):
        # Exercise actual Rust mapping through Pyodide, not Python arithmetic.
        for limits in ((1e16, 1e16 + 2, 2), (-1e16 - 2, -1e16, 2)):
            offset = Axes(limits, limits, x_length=8, y_length=4, include_ticks=False)
            near(offset.c2p(limits[0], limits[0]), (-4, -2))
            near(offset.c2p(limits[1], limits[1]), (4, 2))
            mapped = offset.plot_samples(((limits[0], limits[0]), (limits[1], limits[1])))
            near(mapped.get_start(), (-4, -2))
            near(mapped.get_end(), (4, 2))
            timed = offset.time_series_plan(((limits[0], limits[0]), (limits[1], limits[1])), run_time=1)
            near(timed.cursor_points[0], (-4, 0))
            near(timed.cursor_points[1], (4, 0))
            offset.shift(RIGHT)
            near(offset.c2p(limits[0], limits[0]), (-3, -2))
            shifted = offset.time_series_plan(((limits[0], limits[0]), (limits[1], limits[1])), run_time=1)
            near(shifted.cursor_points[0], (-3, 0))
            near(timed.cursor_points[0], (-4, 0))

        # Extreme parameter values may still map to ordinary visible geometry.
        # The callback must see both exact endpoints, once each, in that order.
        endpoint_visits = []
        for limits in ((0, 1e-300, 1e100), (0, 5e-324, 2), (1e308, 1.25e308, 1e308)):
            visited = []
            def normalized(t):
                visited.append(t)
                alpha = (t - limits[0]) / (limits[1] - limits[0])
                return (alpha, alpha)
            boundary_curve = ParametricFunction(normalized, limits, use_smoothing=False)
            assert visited == [limits[0], limits[1]], visited
            near(boundary_curve.get_start(), (0, 0))
            near(boundary_curve.get_end(), (1, 1))
            assert boundary_curve.get_num_curves() == 1
            endpoint_visits.append(visited)

        line = NumberLine([2, 6, 1], length=8, rotation=pi / 2)
        near(line.n2p(4), (0, 0))
        near(line.n2p(6), (0, 4))
        assert abs(line.p2n((0, 2)) - 5) < 2e-5
        assert abs(line.get_unit_size() - 2) < 2e-5
        assert len(line.ticks.submobjects) == 5
        copied_line = line.copy().shift(RIGHT)
        assert copied_line.x_range == line.x_range
        near(copied_line.n2p(4), (1, 0))
        near(line.n2p(4), (0, 0))

        axes = Axes([-2, 2, 1], [-1, 1, 0.5], x_length=6, y_length=3)
        axes.scale(0.8).rotate(0.25).shift(LEFT)
        for point in [(0, 0), (1, 0.5), (-3, 2)]:
            near(axes.p2c(axes.c2p(*point)), point)
        copy = axes.copy().shift(UP)
        near(copy.c2p(0, 0), axes.c2p(0, 0) + UP)

        calls = []
        def function(x):
            calls.append(x)
            return x * x / 2
        curve = axes.plot(function, [-1, 1, 0.5], use_smoothing=False, color=BLUE)
        assert calls == [-1, -0.5, 0, 0.5, 1], calls
        near(curve.get_start(), axes.c2p(-1, 0.5))
        near(curve.get_end(), axes.c2p(1, 0.5))
        data = axes.plot_samples([(1, 0), (1, 1), (-1, 0.25)], color=YELLOW)
        assert data.get_num_curves() == 2
        near(data.get_start(), axes.c2p(1, 0))
        near(data.get_end(), axes.c2p(-1, 0.25))

        split = FunctionGraph(lambda x: 1 / x, [-1, 1, 0.1],
                              discontinuities=[0], dt=0.05, use_smoothing=False)
        assert len(split.get_subpaths()) == 2
        parametric = ParametricFunction(lambda t: (cos(t), sin(t), 0), [0, pi, 0.1])
        near(parametric.get_start(), (1, 0))
        near(parametric.get_end(), (-1, 0))

        failure = RuntimeError("intentional plot evaluation failure")
        def rejected_function(x):
            raise failure
        try:
            axes.plot(rejected_function)
        except RuntimeError as caught:
            assert caught is failure
        else:
            raise AssertionError("callback failure was swallowed")
        try:
            axes.plot(lambda x: float('nan'))
        except NoonValueError:
            pass
        else:
            raise AssertionError("nonfinite plot sample was accepted")
        try:
            NumberLine([0, 1, 0])
        except NoonValueError:
            pass
        else:
            raise AssertionError("invalid coordinate range was accepted")

        sentinel = Dot((4, 2), color=GREEN)
        self.add(axes, data, sentinel)
        self.play(Create(curve), run_time=0.2, rate_func=linear)
        origin = axes.c2p(0, 0)
        self.play(axes.animate.shift(RIGHT), run_time=0.2, rate_func=linear)
        near(axes.c2p(0, 0), origin + RIGHT)
        near(axes.p2c(axes.c2p(0.75, 0.25)), (0.75, 0.25))
        near(sentinel.get_center(), (4, 2))
        assert len(calls) == 5, "static curve was reevaluated during playback"

        # Resumed authoring must sample against the completed effective axes and
        # publish a new detached path through the same retained execution owner.
        later = axes.plot(lambda x: -x / 2, [-1, 1, 0.5], color=RED)
        near(later.get_start(), axes.c2p(-1, 0.5))
        self.play(Create(later), run_time=0.2, rate_func=linear)
        near(sentinel.get_center(), (4, 2))
        assert len(calls) == 5
        area = axes.get_area(later, [-0.75, 0.75], color=GREEN, opacity=0.4)
        rectangles = axes.get_riemann_rectangles(later, [-1, 1], dx=0.5, input_sample_type="center", fill_opacity=0.6)
        self.add(area, rectangles)
        near(area.get_start(), axes.c2p(-0.75, 0))
        assert len(rectangles.submobjects) == 4
        assert abs(rectangles.submobjects[0].get_fill_opacity() - 0.6) < 1e-6
        self.remove(area, rectangles)
        assert len(calls) == 5, "area queries must not reevaluate unrelated callbacks"
        for bad_length in (float("nan"), -1, 0):
            try:
                Axes([-1, 1, 1], [-1, 1, 1], x_length=2, y_length=bad_length)
            except NoonValueError:
                pass
            else:
                raise AssertionError("invalid live axis length was accepted")
        fresh = Axes([-1, 1, 1], [-1, 1, 1], x_length=2, y_length=2)
        fresh.shift(LEFT)
        line = NumberLine([0, 2, 1], length=2).shift(DOWN)
        for observe in (lambda: fresh.c2p(0, 0), lambda: line.n2p(1)):
            try:
                observe()
            except NotImplementedError:
                pass
            else:
                raise AssertionError("unsupported detached live query was silently accepted")
        self.add(fresh, line)
        near(fresh.c2p(0, 0), LEFT)
        near(fresh.p2c(fresh.c2p(0.5, 0.25)), (0.5, 0.25))
        assert abs(line.p2n(line.n2p(1)) - 1) < 2e-5
        self.play(fresh.animate.shift(UP), run_time=0.2, rate_func=linear)
        near(fresh.c2p(0, 0), LEFT + UP)
        self.remove(fresh, line)
        near(sentinel.get_center(), (4, 2))
        assert all(len(visited) == 2 for visited in endpoint_visits)
`;

export async function qualifyPlotting(runLive) {
  directWasmPreparation();
  const result = await runLive(source);
  if (Math.abs(result.duration - 0.8) > 1e-6 || result.metrics.objectCount !== 14) {
    throw new Error(`plotting lifecycle produced unexpected duration/membership: ${JSON.stringify(result)}`);
  }
  if (!result.metrics.ready || !result.metrics.retained || result.metrics.presentedFrames < 1 ||
      result.metrics.drawCalls < 1 || result.metrics.instancesDrawn < 1) {
    throw new Error("plotting did not present through the retained renderer");
  }
  const numberLabels = await qualifyNumberLabels(runLive);
  return { numberLabels, backend: result.metrics.backend, objectCount: result.metrics.objectCount,
    presentedFrames: result.metrics.presentedFrames, duration: result.duration };
}
