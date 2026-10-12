from manim import *


class RetainedAreaHelpers(Scene):
    def construct(self):
        axes = Axes([-3, 3, 1], [-2, 3, 1], x_length=8, y_length=5, tips=False)
        graph = axes.plot(lambda x: 0.5 * x, [-3, 3, 0.25], use_smoothing=False)
        lower = axes.plot(lambda x: -0.5, [-3, 3, 0.25], use_smoothing=False)
        area = axes.get_area(graph, [-2.5, -0.5], color=GREEN, opacity=0.4, stroke_width=0)
        rectangles = axes.get_riemann_rectangles(graph, [-0.5, 2.5], dx=0.5, input_sample_type="center", stroke_width=0)
        bounded = axes.get_area(graph, [1, 2], bounded_graph=lower, color=YELLOW, opacity=0.3, stroke_width=0)
        self.add(area, rectangles, bounded)
        self.wait(0.2)
