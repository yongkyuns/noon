"""Two scalar fields, one captured Axes frame, two static retained paths.

Paired with noon::example_scenes::implicit_plotting on native and direct WASM.
The callbacks run only while the contours are constructed.
"""
from noon import *


class ImplicitPlotting(Scene):
    def construct(self):
        axes = Axes([-3, 3, 1], [-2, 2, 1], x_length=9, y_length=4.8)
        axes.shift(0.15 * DOWN)
        circle = axes.plot_implicit_curve(
            lambda x, y: x * x + y * y - 2.25,
            min_depth=4, max_quads=600, color=BLUE,
        )
        hyperbola = axes.plot_implicit_curve(
            lambda x, y: x * y - 0.65,
            min_depth=4, max_quads=600, color=YELLOW,
        )
        title = Text("Implicit curves: sampled once, retained", font_size=28).shift(3.1 * UP)
        caption = Text("x² + y² = 2.25     |     xy = 0.65", font_size=24).shift(3.1 * DOWN)
        self.add(axes, circle, hyperbola, title, caption)
