from noon import *


class GraphStyleExample(Scene):
    def construct(self):
        vertices = ["a", "b", "c"]
        edges = [("a", "b"), ("b", "c"), ("c", "a")]
        options = dict(
            vertex_config={"radius": 0.21, "fill_color": RED,
                           "fill_opacity": 0.35, "stroke_color": BLUE,
                           "stroke_width": 3, "stroke_opacity": 0.45,
                           "b": {"stroke_opacity": 0.2}},
            edge_config={"color": GREEN, "stroke_width": 6,
                         "stroke_opacity": 0.55,
                         ("a", "b"): {"stroke_opacity": 0.3}},
        )
        left = Graph(vertices, edges, labels={"a": Text("a", color=BLACK)}, layout={
            "a": [-5, -1, 0], "b": [-3.5, 1.2, 0], "c": [-2, -1, 0],
        }, **options)
        right = DiGraph(vertices, edges, labels={"a": Text("a", color=BLACK)}, layout={
            "a": [2, -1, 0], "b": [3.5, 1.2, 0], "c": [5, -1, 0],
        }, **options)
        self.add(left, right)
        self.wait(0.2)
