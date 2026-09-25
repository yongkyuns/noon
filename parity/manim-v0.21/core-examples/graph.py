from manim import *


class GraphExample(Scene):
    def construct(self):
        vertices = ["a", "b", "c"]
        edges = [("a", "b"), ("b", "c"), ("c", "a")]
        left = Graph(vertices, edges, layout={
            "a": [-5, -1, 0], "b": [-3.5, 1.2, 0], "c": [-2, -1, 0],
        })
        right = DiGraph(vertices, edges, layout={
            "a": [2, -1, 0], "b": [3.5, 1.2, 0], "c": [5, -1, 0],
        })
        self.add(left, right)
        self.wait(0.2)
