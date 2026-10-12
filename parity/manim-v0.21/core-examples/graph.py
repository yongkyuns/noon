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


class GraphStyleExample(Scene):
    def construct(self):
        vertices = ["a", "b", "c"]
        edges = [("a", "b"), ("b", "c"), ("c", "a")]
        options = dict(
            vertex_config={"radius": 0.21, "fill_color": RED,
                           "fill_opacity": 0.35, "stroke_color": BLUE,
                           "stroke_width": 3},
            edge_config={"color": GREEN, "stroke_width": 6},
        )
        left = Graph(vertices, edges, layout={
            "a": [-5, -1, 0], "b": [-3.5, 1.2, 0], "c": [-2, -1, 0],
        }, **options)
        right = DiGraph(vertices, edges, layout={
            "a": [2, -1, 0], "b": [3.5, 1.2, 0], "c": [5, -1, 0],
        }, **options)
        self.add(left, right)
        self.wait(0.2)
