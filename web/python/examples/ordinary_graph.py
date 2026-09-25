"""Graph/DiGraph topology edits and shared circular/explicit layouts."""

from noon import DiGraph, Graph, Scene


class OrdinaryGraph(Scene):
    def construct(self):
        self.wait(0.01)
        graph = Graph({"a": (-5, -1), "b": (-3.5, 1.2), "c": (-2, -1)},
                      [("a", "b"), ("b", "c"), ("c", "a")], layout="explicit")
        graph.change_layout("explicit", positions={
            "a": (-5, -1), "b": (-3.5, 1.2), "c": (-2, -1),
        })
        graph.add_vertices({"d": (-3.5, -2.4)}).add_edges([("a", "d")])

        directed = DiGraph({"u": (2, -1), "v": (3.5, 1.2), "w": (5, -1)},
                           [("u", "v"), ("v", "w"), ("w", "u")], layout="explicit")
        directed.change_layout("circular", scale=1.6, center=(3.5, 0))
        directed.add_vertices({"x": (5, -2.3)}).add_edges([("w", "x")])
        self.add(graph, directed)
        self.wait(0.2)
