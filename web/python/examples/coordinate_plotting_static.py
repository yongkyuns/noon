"""Paired with noon::coordinate_plotting_example.

Explicit linear ranges, a sampled function and an illustrative data series.
Curves are independent retained paths; group them with axes to move them together.
"""
from noon import *
from math import sin


class CoordinatePlotting(Scene):
    def construct(self):
        axes = Axes(
            [0, 10, 2], [-1.5, 1.5, 0.5], x_length=10, y_length=4,
        )
        curve = axes.plot(lambda t: sin(0.8 * t), [0, 10, 0.05], color=BLUE)
        samples = [
            (0, 0.1), (2, 0.95), (4, -0.1),
            (6, -0.9), (8, 0.2), (10, 1.0),
        ]
        data = axes.plot_samples(samples, color=YELLOW)
        title = Text("Shared axes: function and sampled data", font_size=28).shift(3 * UP)
        label = Text("Time (s)", font_size=22).shift(2.7 * DOWN)
        self.add(axes, curve, data, title, label)
