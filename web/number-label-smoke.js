// Actual Pyodide object lifecycle and direct WASM admission tests. Numeric
// rendering is additionally paired in the shared time-series example.
import { WasmCoordinateOptions } from "./pkg/noon_web.js";

const source = `
from noon import *
from math import pi

def near(a, b):
    assert all(abs(x-y) < 2e-5 for x, y in zip(a, b)), (a, b)

class NumericLabelQualification(Scene):
    def construct(self):
        line = NumberLine((-2, 2, 1), length=4)
        detached = line.get_number_mobjects(1, -1, 1)
        assert tuple(label.source for label in detached) == ("1", "-1", "1")
        assert len(line.submobjects) == 2
        assert line.add_numbers((), exclude_zero=False) is line
        assert len(line.numbers) == 0
        empty = line.numbers
        assert line.add_numbers((1, -1, 1), color=BLUE) is line
        assert len(line.submobjects) == 4
        assert empty is line.submobjects[2]
        assert line.numbers is line.submobjects[3]
        assert all(isinstance(label, Text) for label in line.numbers)
        old_center = line.numbers[0].get_center()
        copy = line.copy().shift(RIGHT)
        assert copy.numbers is copy.submobjects[3]
        assert copy.numbers is not line.numbers
        near(copy.numbers[0].get_center(), old_center + RIGHT)
        near(line.numbers[0].get_center(), old_center)
        near(detached[0].get_center(), old_center)

        axes = Axes((0, 4, 1), (0, 2, 1), x_length=4, y_length=2)
        before = (len(axes.x_axis), len(axes.y_axis))
        try:
            axes.add_coordinates(y_config={"font": "No such font family"})
        except ValueError:
            pass
        else:
            raise AssertionError("invalid second-axis font accepted")
        assert (len(axes.x_axis), len(axes.y_axis)) == before
        assert not hasattr(axes.x_axis, "numbers")
        assert axes.add_coordinates((0, 2, 4), (1, 2), exclude_zero=False) is axes
        assert tuple(label.source for label in axes.x_axis.numbers) == ("0", "2", "4")
        assert tuple(label.source for label in axes.y_axis.numbers) == ("1", "2")
        center = axes.x_axis.numbers[1].get_center()
        axes.shift(UP)
        near(axes.x_axis.numbers[1].get_center(), center + UP)
        axes.scale(0.8).rotate(pi/8)
        near(axes.p2c(axes.c2p(2, 1)), (2, 1))
        copy = axes.copy().shift(RIGHT)
        assert copy.x_axis.numbers is not axes.x_axis.numbers
        near(copy.x_axis.numbers[0].get_center(), axes.x_axis.numbers[0].get_center() + RIGHT)
        for options in ({"decimal_places": 0.5}, {"decimal_places": True}, {"label_constructor": Text}):
            try:
                line.add_numbers((1,), **options)
            except TypeError:
                pass
            else:
                raise AssertionError("unsupported label option accepted")
        sentinel = Circle(radius=0.1).move_to((4, 2))
        self.add(axes, sentinel)
        labels = tuple(axes.x_axis.numbers) + tuple(axes.y_axis.numbers)
        centers = tuple(label.get_center() for label in labels)
        origin = axes.c2p(0, 0)
        self.play(axes.animate.shift(RIGHT), run_time=0.2, rate_func=linear)
        for label, expected in zip(labels, centers):
            near(label.get_center(), expected + RIGHT)
        near(axes.c2p(0, 0), origin + RIGHT)
        near(sentinel.get_center(), (4, 2))
        try:
            axes.add_coordinates()
        except NotImplementedError:
            pass
        else:
            raise AssertionError("live label construction silently used authored state")
`;

export async function qualifyNumberLabels(runLive) {
  for (const precision of [-1, 0.5, NaN, Infinity, 13, 2**32]) {
    let rejected = false;
    try { WasmCoordinateOptions.numberLabels("DejaVu Sans Mono", 18, precision, 0.12); }
    catch (error) { rejected = error.category === "invalid_input"; }
    if (!rejected) throw new Error(`invalid raw precision accepted: ${precision}`);
  }
  const result = await runLive(source);
  if (Math.abs(result.duration - 0.2) > 1e-6 || !result.metrics.ready ||
      !result.metrics.retained || result.metrics.presentedFrames < 1) {
    throw new Error("numeric label lifecycle did not complete and present");
  }
  return { duration: result.duration, backend: result.metrics.backend,
    objectCount: result.metrics.objectCount, presentedFrames: result.metrics.presentedFrames };
}
