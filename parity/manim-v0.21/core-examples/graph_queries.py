"""Callable graph-query oracle for ManimCE 0.21.0; not FollowingGraphCamera.

Semantics: manim/mobject/graphing/coordinate_systems.py at v0.21.0.
Only query results are displayed, independent of default axis appearance.
"""
from manim import *


class CallableGraphQueries(Scene):
    def construct(self):
        axes = Axes([-2, 2, 1], [-1, 1, 1], x_length=4, y_length=2, tips=False)
        graph = axes.plot(lambda x: 0.5 * x * x, [-1, 1, 0.25], use_smoothing=False)
        receiver = Axes([-2, 2, 1], [-1, 1, 1], x_length=4, y_length=2, tips=False)
        receiver.shift(3 * RIGHT)
        start = receiver.i2gp(graph.t_min, graph)
        end = receiver.input_to_graph_point(graph.t_max, graph)
        axes.shift(0.75 * UP)
        current = graph.function(0)
        copied = graph.copy()
        replacement = axes.plot(lambda x: -x, [2, 5, 1], use_smoothing=False)
        copied.become(replacement)
        # become changes appearance, preserving the receiver's callable/range.
        copy_start = receiver.i2gp(copied.t_min, copied)
        copy_end = receiver.i2gp(copied.t_max, copied)
        world_graph = FunctionGraph(lambda x: -2 + 0.25 * x, [-1, 1, 0.5],
                                    use_smoothing=False)
        for point, color in ((start, ORANGE), (end, YELLOW), (current, RED),
                             (world_graph.function(world_graph.t_min), ORANGE),
                             (world_graph.function(world_graph.t_max), YELLOW),
                             (copy_start, GREEN), (copy_end, PURPLE)):
            self.add(Dot(point, color=color))
        self.wait(0.2)
