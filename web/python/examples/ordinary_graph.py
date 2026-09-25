"""Paired Graph/DiGraph source: circular + explicit layouts and live topology edits."""

from noon import DiGraph, Graph, Scene


class OrdinaryGraph(Scene):
    def construct(self):
        # Enter a returned live session before constructing opaque Graph handles.
        self.wait(0.01)
        graph = Graph({"a": (-4, -1), "b": (-2.5, 1), "c": (-1, -1)},
                      [("a", "b"), ("b", "c")], layout="circular", layout_scale=1.4,
                      layout_center=(-3, 0))
        graph.add_vertices({"d": (-3, -2)}).add_edges([("c", "d")])
        graph.change_layout("explicit", positions={
            "a": (-4, -1), "b": (-2.5, 1), "c": (-1, -1), "d": (-3, -2),
        })
        self.add(graph)

        directed = DiGraph({"u": (2, -1), "v": (3.5, 1), "w": (5, -1)},
                           [("u", "v"), ("v", "w")], layout="circular",
                           layout_scale=1.4, layout_center=(3.5, 0))
        directed.add_vertices({"x": (3.5, -2)}).add_edges([("w", "x")])
        self.add(directed)
        self.wait(0.2)
