"""Paired with noon::example_scenes::graph_queries on native and direct WASM."""
from noon import *


class GraphQueries(Scene):
    def construct(self):
        axes = Axes([-2, 2, 1], [-1, 1, 1], x_length=4, y_length=2, tips=False)
        graph = axes.plot(lambda x: 0.5 * x * x, [-1, 1, 0.25],
                          color=BLUE, use_smoothing=False)
        start = axes.i2gp(graph.t_min, graph)
        end = axes.input_to_graph_point(graph.t_max, graph)
        # The callable uses the creating axes' current frame. The retained blue
        # curve stays where it was authored; a query does not resample it.
        axes.shift(0.75 * UP)
        current = graph.function(0)

        # A standalone FunctionGraph's callable returns scene coordinates.
        world_graph = FunctionGraph(lambda x: -2 + 0.25 * x, [-1, 1, 0.5],
                                    color=GREEN, use_smoothing=False)
        self.add(axes, graph, world_graph)
        for point, color in ((start, ORANGE), (end, YELLOW), (current, RED),
                             (world_graph.function(world_graph.t_min), ORANGE),
                             (world_graph.function(world_graph.t_max), YELLOW)):
            self.add(Dot(point, color=color))
