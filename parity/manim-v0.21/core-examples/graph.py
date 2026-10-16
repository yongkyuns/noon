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


class GraphCircleVertexType(Scene):
    def construct(self):
        graph = Graph(
            ["circle-left", "circle-right"],
            [("circle-left", "circle-right")],
            layout={"circle-left": [-1.4, 0, 0], "circle-right": [1.4, 0, 0]},
            vertex_type=Circle,
        )
        self.add(graph)
        self.wait(0.2)


class GraphDefaultSpringLayout(Scene):
    def construct(self):
        graph = Graph(
            ["a", "b", "c"],
            [("a", "b"), ("b", "c")],
            layout_config={"seed": 42},
        )
        self.add(graph)
        self.wait(0.2)


class DiGraphDefaultSpringLayout(Scene):
    def construct(self):
        graph = DiGraph(
            ["a", "b", "c"],
            [("a", "b"), ("c", "b")],
            layout_config={"seed": 42},
        )
        self.add(graph)
        self.wait(0.2)


class GraphWeightedSpringLayout(Scene):
    def construct(self):
        graph = Graph(
            ["a", "b", "c"],
            [("a", "b"), ("b", "c")],
            layout_config={"seed": 42},
        )
        # ManimCE 0.21.0 later hashes each two-item edge as an edge_config key,
        # so three-item NetworkX edge rows containing dicts fail in its Graph
        # constructor. Seed the owned NetworkX graph, then exercise its public
        # layout entry point with the selected attribute weight.
        graph._graph["a"]["b"]["strength"] = 3.0
        graph._graph["b"]["c"]["strength"] = 0.4
        graph.change_layout(layout="spring", layout_config={"seed": 42, "weight": "strength"})
        self.add(graph)
        self.wait(0.2)


class DiGraphWeightedSpringLayout(Scene):
    def construct(self):
        graph = DiGraph(
            ["a", "b", "c"],
            [("a", "b"), ("c", "b")],
            layout_config={"seed": 42},
        )
        graph._graph["a"]["b"]["strength"] = 2.5
        graph._graph["c"]["b"]["strength"] = 0.6
        graph.change_layout(layout="spring", layout_config={"seed": 42, "weight": "strength"})
        self.add(graph)
        self.wait(0.2)


class GraphSeededRandomLayout(Scene):
    def construct(self):
        graph = Graph(
            ["a", "b", "c"],
            [("a", "b"), ("b", "c"), ("c", "a")],
            layout="random",
            layout_config={"seed": 42},
        )
        self.add(graph)
        self.wait(0.2)


class DiGraphSeededRandomLayout(Scene):
    def construct(self):
        graph = DiGraph(
            ["a", "b", "c"],
            [("a", "b"), ("c", "b")],
            layout="random",
            layout_config={"seed": 42},
        )
        self.add(graph)
        self.wait(0.2)


class GraphCircularLayout(Scene):
    def construct(self):
        graph = Graph(
            ["a", "b", "c", "d"],
            [("a", "b"), ("b", "c"), ("c", "d")],
            layout="circular",
            layout_config={"center": (0.35, -0.2), "dim": 2},
        )
        self.add(graph)
        self.wait(0.2)


class DiGraphCircularLayout(Scene):
    def construct(self):
        graph = DiGraph(
            ["a", "b", "c", "d"],
            [("a", "b"), ("c", "b"), ("c", "d")],
            layout="circular",
            layout_config={"center": (-0.25, 0.4), "dim": 2},
        )
        self.add(graph)
        self.wait(0.2)


class GraphShellLayout(Scene):
    def construct(self):
        graph = Graph(
            ["a", "b", "c", "d"],
            [("a", "b"), ("b", "c"), ("c", "d")],
            layout="shell",
            layout_config={"nlist": [["a", "b"], ["c", "d"]], "rotate": 0.3,
                           "center": (0.2, -0.3), "dim": 2},
        )
        self.add(graph)
        self.wait(0.2)


class DiGraphShellLayout(Scene):
    def construct(self):
        graph = DiGraph(
            ["a", "b", "c", "d"],
            [("a", "b"), ("c", "b"), ("c", "d")],
            layout="shell",
            layout_config={"nlist": [["a", "c"], ["b", "d"]], "rotate": -0.2,
                           "center": (-0.3, 0.25), "dim": 2},
        )
        self.add(graph)
        self.wait(0.2)


class GraphSpiralLayout(Scene):
    def construct(self):
        graph = Graph(
            ["a", "b", "c", "d"],
            [("a", "b"), ("b", "c"), ("c", "d")],
            layout="spiral",
            layout_config={"resolution": 0.5, "equidistant": True,
                           "center": (0.3, -0.25), "dim": 2},
        )
        self.add(graph)
        self.wait(0.2)


class DiGraphSpiralLayout(Scene):
    def construct(self):
        graph = DiGraph(
            ["a", "b", "c", "d"],
            [("a", "b"), ("c", "b"), ("c", "d")],
            layout="spiral",
            layout_config={"resolution": 0.4, "equidistant": False,
                           "center": (-0.2, 0.3), "dim": 2},
        )
        self.add(graph)
        self.wait(0.2)


class GraphPartiteLayout(Scene):
    def construct(self):
        graph = Graph(
            ["a", "b", "c", "d"],
            [("a", "c"), ("a", "d"), ("b", "c")],
            layout="partite", partitions=[["a", "b"], ["c", "d"]],
            layout_config={"align": "horizontal", "center": (0.25, -0.2)},
        )
        self.add(graph)
        self.wait(0.2)


class DiGraphPartiteLayout(Scene):
    def construct(self):
        graph = DiGraph(
            ["a", "b", "c", "d"],
            [("a", "c"), ("b", "c"), ("b", "d")],
            layout="partite", partitions=[["a", "b"], ["c", "d"]],
            layout_config={"align": "vertical", "center": (-0.2, 0.25)},
        )
        self.add(graph)
        self.wait(0.2)


class GraphSpectralLayout(Scene):
    def construct(self):
        graph = Graph(
            ["a", "b", "c", "d"],
            [("a", "b"), ("b", "c"), ("c", "d")],
            layout="spectral",
            layout_config={"weight": None, "center": (0.3, -0.25), "dim": 2},
        )
        self.add(graph)
        self.wait(0.2)


class DiGraphSpectralLayout(Scene):
    def construct(self):
        graph = DiGraph(
            ["a", "b", "c", "d"],
            [("a", "b"), ("c", "b"), ("c", "d")],
            layout="spectral",
            layout_config={"weight": None, "center": (-0.25, 0.3), "dim": 2},
        )
        self.add(graph)
        self.wait(0.2)


class GraphKamadaKawaiLayout(Scene):
    def construct(self):
        graph = Graph(
            ["a", "b", "c", "d"],
            [("a", "b"), ("b", "c"), ("c", "d")],
            layout="kamada_kawai",
            layout_config={"weight": None, "center": (0.25, -0.3), "dim": 2},
        )
        self.add(graph)
        self.wait(0.2)


class DiGraphKamadaKawaiLayout(Scene):
    def construct(self):
        graph = DiGraph(
            ["a", "b", "c", "d"],
            [("a", "b"), ("c", "b"), ("c", "d")],
            layout="kamada_kawai",
            layout_config={"weight": None, "center": (-0.3, 0.25), "dim": 2},
        )
        self.add(graph)
        self.wait(0.2)


class GraphTreeLayout(Scene):
    def construct(self):
        graph = Graph(
            ["root", "left", "right", "leaf"],
            [("root", "left"), ("root", "right"), ("left", "leaf")],
            layout="tree", root_vertex="root",
            layout_config={"orientation": "down", "vertex_spacing": (1.1, 0.8)},
        )
        self.add(graph)
        self.wait(0.2)


class GraphDefaultLabels(Scene):
    def construct(self):
        graph = Graph(
            ["a", "b", "c"], [("a", "b"), ("b", "c"), ("c", "a")],
            layout="circular", layout_scale=1.3,
            layout_config={"center": (-1.2, 0.2)},
            labels=True, label_fill_color=YELLOW,
        )
        graph.add_vertices("d", positions={"d": (1.5, 0.4)}, labels=True)
        graph.change_layout(
            "circular", scale=1.0, layout_config={"center": (-0.8, -0.1)},
        )
        copied = graph.copy().shift(3 * RIGHT)
        graph.remove_vertices("b")
        self.add(graph, copied)
        self.wait(0.2)


class DiGraphPartialLabels(Scene):
    def construct(self):
        graph = DiGraph(
            ["a", "b", "c"], [("a", "b"), ("c", "b")],
            layout={"a": [-2.5, 0.4, 0], "b": [0, -0.4, 0], "c": [2.5, 0.4, 0]},
            labels={"a": MathTex(r"\alpha", color=GREEN), "c": Text("C", color=ORANGE)},
        )
        graph.add_vertices(
            "d", positions={"d": (2.0, -1.6)},
            labels={"d": MathTex("d", color=BLUE)},
        )
        graph.change_layout(
            "circular", scale=1.2, layout_config={"center": (0.2, -0.1)},
        )
        copied = graph.copy().shift(3 * LEFT)
        graph.remove_vertices("b")
        self.add(graph, copied)
        self.wait(0.2)
