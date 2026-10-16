"""Graph/DiGraph topology edits and shared circular/explicit layouts."""

from noon import BLUE, GREEN, PURPLE, RED, YELLOW, DiGraph, Graph, Scene


class OrdinaryGraph(Scene):
    def construct(self):
        self.wait(0.01)
        styles = dict(
            vertex_config={"radius": 0.21, "fill_color": RED, "fill_opacity": 0.35,
                           "stroke_color": BLUE, "stroke_width": 3},
            edge_config={"color": GREEN, "stroke_width": 6},
        )
        graph_styles = dict(
            vertex_config={**styles["vertex_config"], "b": {"fill_color": YELLOW}},
            edge_config={**styles["edge_config"], ("a", "b"): {"color": PURPLE}},
        )
        directed_styles = dict(
            vertex_config={**styles["vertex_config"], "v": {"fill_color": YELLOW}},
            edge_config={**styles["edge_config"], ("v", "w"): {"color": PURPLE}},
        )
        graph = Graph({"a": (-5, -1), "b": (-3.5, 1.2), "c": (-2, -1)},
                      [("a", "b"), ("b", "c"), ("c", "a")], layout="explicit", **graph_styles)
        graph.change_layout("explicit", positions={
            "a": (-5, -1), "b": (-3.5, 1.2), "c": (-2, -1),
        })
        graph.add_vertices({"d": (-3.5, -2.4)}).add_edges([("a", "d")])

        directed = DiGraph({"u": (2, -1), "v": (3.5, 1.2), "w": (5, -1)},
                           [("u", "v"), ("v", "w"), ("w", "u")], layout="explicit", **directed_styles)
        directed.change_layout("circular", scale=1.6, center=(3.5, 0))
        directed.add_vertices({"x": (5, -2.3)}).add_edges([("w", "x")])
        centered = Graph(
            ["left", "right"], [("left", "right")], layout="circular",
            layout_scale=1.4, layout_config={"center": (3, -1), "dim": 2},
        )
        left = centered["left"].get_center()
        right = centered["right"].get_center()
        assert abs(left.x - 4.4) < 1e-6 and abs(left.y + 1.0) < 1e-6
        assert abs(right.x - 1.6) < 1e-6 and abs(right.y + 1.0) < 1e-6
        self.add(graph, directed, centered)
        self.wait(0.2)
