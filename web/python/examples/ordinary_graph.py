"""Graph/DiGraph topology edits and shared spring/random/tree/circular/shell/spiral/partite layouts."""

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
            vertex_config={**styles["vertex_config"], "b": {"fill_color": YELLOW, "radius": 0.28}},
            edge_config={**styles["edge_config"], ("a", "b"): {"color": PURPLE}},
        )
        directed_styles = dict(
            vertex_config={**styles["vertex_config"], "v": {"fill_color": YELLOW, "radius": 0.28}},
            edge_config={**styles["edge_config"], ("v", "w"): {"color": PURPLE}},
        )
        graph = Graph({"a": (-5, -1), "b": (-3.5, 1.2), "c": (-2, -1)},
                      [("a", "b"), ("b", "c"), ("c", "a")], layout="explicit", **graph_styles)
        assert tuple(graph.edges) == (("a", "b"), ("a", "c"), ("b", "c"))
        _ = graph[("a", "b")]
        graph.change_layout("explicit", positions={
            "a": (-5, -1), "b": (-3.5, 1.2), "c": (-2, -1),
        })
        graph.add_edges(
            [("a", "d")],
            positions={"d": (-3.5, -2.4)},
            vertex_config={"d": {"radius": 0.16, "fill_color": YELLOW, "stroke_width": 5}},
            edge_config={("a", "d"): {"color": PURPLE, "stroke_width": 3}},
        )
        assert ("a", "d") in graph.edges
        added_vertices = graph.add_vertices(
            "e", positions={"e": (-5.5, -2.4)},
            vertex_config={"e": {"radius": 0.16, "fill_color": YELLOW, "stroke_width": 5}},
        )
        assert len(added_vertices) == 1 and added_vertices[0] is graph["e"]

        directed = DiGraph({"u": (2, -1), "v": (3.5, 1.2), "w": (5, -1)},
                           [("u", "v"), ("v", "w"), ("w", "u")], layout="explicit", **directed_styles)
        assert tuple(directed.edges) == (("u", "v"), ("v", "w"), ("w", "u"))
        _ = directed[("u", "v")]
        directed.change_layout("circular", scale=(1.6, 1.0), center=(3.5, 0))
        directed.add_edges(
            [("w", "x")],
            positions={"x": (5, -2.3)},
            vertex_config={"x": {"color": YELLOW}},
            edge_config={("w", "x"): {"color": PURPLE, "stroke_width": 3}},
        )
        assert ("w", "x") in directed.edges
        added_vertices = directed.add_vertices(
            "y", positions={"y": (2, -2.3)},
            vertex_config={"y": {"radius": 0.16, "color": YELLOW}},
        )
        assert len(added_vertices) == 1 and added_vertices[0] is directed["y"]
        centered = Graph(
            ["left", "right"], [("left", "right")], layout="circular",
            layout_scale=(1.4, 0.8), layout_config={"center": (3, -1), "dim": 2},
        )
        left = centered["left"].get_center()
        right = centered["right"].get_center()
        assert abs(left.x - 4.4) < 1e-6 and abs(left.y + 1.0) < 1e-6
        assert abs(right.x - 1.6) < 1e-6 and abs(right.y + 1.0) < 1e-6
        shells = [["s0", "s1"], ["s2"]]
        shell_graph = Graph(
            ["s0", "s1", "s2"], [("s0", "s1"), ("s1", "s2")],
            layout="shell", layout_scale=0.9,
            layout_config={"nlist": shells, "center": (0, 2.6)},
            **styles,
        )
        shell_graph.change_layout(
            "shell", scale=0.9,
            layout_config={"nlist": shells, "center": (0, 2.6)},
        )
        spiral_graph = Graph(
            ["p0", "p1", "p2"], [("p0", "p1"), ("p1", "p2")],
            layout="spiral", layout_scale=(0.9, 0.55),
            layout_config={
                "resolution": 0.35, "equidistant": True, "center": (0, -2.6),
            },
            **styles,
        )
        spiral_graph.change_layout(
            "spiral", scale=(0.9, 0.55),
            layout_config={
                "resolution": 0.35, "equidistant": True, "center": (0, -2.6),
            },
        )
        partitions = [["q0"], ["q1", "q2"]]
        partite_graph = Graph(
            ["q0", "q1", "q2"], [("q0", "q1"), ("q1", "q2")],
            layout="partite", layout_scale=(0.9, 0.55), partitions=partitions,
            layout_config={
                "align": "horizontal", "center": (3.5, 2.6),
            },
            **styles,
        )
        partite_graph.change_layout(
            "partite", scale=(0.9, 0.55), partitions=partitions,
            layout_config={
                "align": "horizontal", "center": (3.5, 2.6),
            },
        )
        spring_graph = Graph(
            ["r0", "r1", "r2"], [("r0", "r1"), ("r1", "r2")],
            layout="spring", layout_scale=(0.65, 0.35),
            layout_config={"seed": 42, "center": (-3.5, 2.6)},
            **styles,
        )
        spring_digraph = DiGraph(
            ["t0", "t1"], [("t0", "t1")],
            layout="spring", layout_scale=0.5,
            layout_config={"seed": 7, "center": (5.5, 2.6)},
            **styles,
        )
        random_graph = Graph(
            ["m0", "m1", "m2"], [("m0", "m1"), ("m1", "m2")],
            layout="random", layout_scale=(1.0, 0.5),
            layout_config={"seed": 17, "center": (2.0, -1.3)},
            **styles,
        )
        random_graph.change_layout(
            "random", scale=(1.0, 0.5),
            layout_config={"seed": 17, "center": (2.0, -1.3)},
        )
        tree_graph = Graph(
            ["z0", "z1", "z2", "z3", "z4"],
            [("z0", "z1"), ("z0", "z2"), ("z1", "z3"), ("z1", "z4")],
            layout="tree", layout_scale=(0.65, 0.35),
            layout_config={"root_vertex": "z0"},
            **styles,
        )
        tree_graph.change_layout(
            "tree", scale=(0.65, 0.35),
            layout_config={"root_vertex": "z0"},
        )
        self.add(
            graph, directed, centered, shell_graph, spiral_graph, partite_graph,
            spring_graph, spring_digraph, random_graph, tree_graph,
        )
        self.wait(0.2)
