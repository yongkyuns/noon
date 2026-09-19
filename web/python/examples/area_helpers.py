"""Paired with noon::example_scenes::area_helpers on native and direct WASM."""
from noon import *


class AreaHelpers(Scene):
    def construct(self):
        axes = Axes([-3, 3, 1], [-2, 4, 1], x_length=8, y_length=5)
        graph = axes.plot(lambda x: 0.25 * x * x - 0.5, [-3, 3, 0.25], color=BLUE, use_smoothing=False)
        area = axes.get_area(graph, [-2, 0], color=GREEN, opacity=0.3)
        rectangles = axes.get_riemann_rectangles(graph, [0, 2], dx=0.25, input_sample_type="center")
        title = Text("Area and Riemann rectangles", font_size=28).shift(3 * UP)
        caption = Text("Area on the left | midpoint samples on the right | negative area inverts color", font_size=16).shift(3 * DOWN)
        self.add(axes, area, rectangles, graph, title, caption)
