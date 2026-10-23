from manim import *


class GraphWeightedSpringLayout(Scene):
    def construct(self):
        graph = Graph(
            ["a", "b", "c"],
            [
                ("a", "b", {"strength": 3.0}),
                ("b", "c", {"strength": 0.4}),
            ],
            layout_config={"seed": 42},
        )
        graph.change_layout(layout="spring", layout_config={"seed": 42, "weight": "strength"})
        self.add(graph)
        self.wait(0.2)


class DiGraphWeightedSpringLayout(Scene):
    def construct(self):
        graph = DiGraph(
            ["a", "b", "c"],
            [
                ("a", "b", {"strength": 2.5}),
                ("c", "b", {"strength": 0.6}),
            ],
            layout_config={"seed": 42},
        )
        graph.change_layout(layout="spring", layout_config={"seed": 42, "weight": "strength"})
        self.add(graph)
        self.wait(0.2)
