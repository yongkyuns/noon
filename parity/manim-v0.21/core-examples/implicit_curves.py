from manim import *


class RetainedImplicitCurves(Scene):
    def construct(self):
        axes = Axes([-2, 2, 1], [-1.5, 1.5, 0.5], x_length=8, y_length=4, tips=False)
        axes.shift(0.2 * DOWN)
        circle = axes.plot_implicit_curve(
            lambda x, y: x * x + y * y - 1,
            min_depth=3, max_quads=200, color=BLUE,
        )
        hyperbola = axes.plot_implicit_curve(
            lambda x, y: x * y - 0.35,
            min_depth=3, max_quads=200, use_smoothing=False, color=YELLOW,
        )
        # Qualify the mapped contours here; coordinate shafts/ticks have their
        # own fixtures, and are not part of implicit geometry.
        self.add(circle, hyperbola)
        self.wait(0.2)
