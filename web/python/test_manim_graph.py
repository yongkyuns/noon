import unittest
from types import SimpleNamespace
from unittest.mock import patch

import _manim_graph as graph


class _Family:
    def memberKeys(self):
        return []


class _Handle:
    def __init__(self, token):
        self.token = token
        self.semanticSlot = token
        self.semanticGeneration = 1
        self.family = lambda: _Family()

    def vertex(self, key):
        return _Handle(key + 100)

    def edge(self, start, end):
        return _Family()


class _GraphOptions:
    def __init__(self):
        self.calls = []
        self.freed = False

    def free(self):
        self.freed = True

    def _record(self, name, *values):
        self.calls.append((name, values))

    def setVertexRadius(self, value): self._record("radius", value)
    def setVertexFill(self, *values): self._record("fill", *values)
    def setVertexFillOpacity(self, value): self._record("fill_opacity", value)
    def setVertexStroke(self, *values): self._record("stroke", *values)
    def setVertexStrokeWidth(self, value): self._record("vertex_width", value)
    def setEdgeColor(self, *values): self._record("edge_color", *values)
    def setEdgeStrokeWidth(self, value): self._record("edge_width", value)


class _Context:
    def __init__(self):
        self.next_token = 1
        self.fail = None
        self.creation_calls = []
        self.layout_calls = []
        self.shell_layout_calls = []
        self.spiral_layout_calls = []
        self.partite_layout_calls = []
        self.spring_layout_calls = []
        self.options = []

    def _call(self, operation):
        if self.fail == operation:
            raise RuntimeError(operation)

    def liveCreateGraph(self, *args):
        self._call("create")
        self.creation_calls.append(args)
        self.options.append(args[-1])
        handle = _Handle(self.next_token)
        self.next_token += 1
        return handle

    liveCreateCircularGraph = liveCreateGraph
    liveCreateShellGraph = liveCreateGraph
    liveCreateSpiralGraph = liveCreateGraph
    liveCreatePartiteGraph = liveCreateGraph
    liveCreateSpringGraph = liveCreateGraph

    def liveGraphCopy(self, handle):
        self._call("copy")
        copied = _Handle(self.next_token)
        self.next_token += 1
        return copied

    def liveGraphAddVertices(self, *args): self._call("add_vertices")
    def liveGraphAddEdges(self, *args): self._call("add_edges")
    def liveGraphRemoveVertices(self, *args): self._call("remove_vertices")
    def liveGraphRemoveEdges(self, *args): self._call("remove_edges")
    def liveGraphCircularLayout(self, *args):
        self._call("circular")
        self.layout_calls.append(args)
    def liveGraphShellLayout(self, *args):
        self._call("shell")
        self.shell_layout_calls.append(args)
    def liveGraphSpiralLayout(self, *args):
        self._call("spiral")
        self.spiral_layout_calls.append(args)
    def liveGraphPartiteLayout(self, *args):
        self._call("partite")
        self.partite_layout_calls.append(args)
    def liveGraphSpringLayout(self, *args):
        self._call("spring")
        self.spring_layout_calls.append(args)
    def liveGraphExplicitLayout(self, *args): self._call("explicit")


class GraphFacadeTests(unittest.TestCase):
    def setUp(self):
        self.context = _Context()
        self.patches = [
            patch.object(graph, "_to_js", side_effect=lambda values: list(values)),
            patch.object(graph, "_graph_options", side_effect=_GraphOptions),
            patch.object(graph._shared, "_live_constructor_context", return_value=self.context),
            patch.object(graph, "engine_call", side_effect=lambda method, *args, **_: method(*args)),
        ]
        for patcher in self.patches: patcher.start()

    def tearDown(self):
        for patcher in reversed(self.patches): patcher.stop()

    def test_default_graph_layout_matches_manim_spring_and_uses_networkx_seed(self):
        graph.Graph(["a", "b", "c"], [("a", "b")], layout_config={"seed": 42})
        call = self.context.creation_calls[0]
        self.assertEqual(call[:3], (False, [0, 1, 2], [0, 1]))
        self.assertEqual(call[4:7], ([], False, 2.0))
        self.assertEqual(call[7:9], (0.0, 0.0))
        self.assertIsNone(call[9])
        self.assertEqual(call[10:], (50, 1e-4, True, self.context.options[0]))
        expected = [
            (0.3745401188473625, 0.9507143064099162),
            (0.7319939418114051, 0.5986584841970366),
            (0.15601864044243652, 0.15599452033620265),
        ]
        for actual, wanted in zip(call[3], (value for point in expected for value in point)):
            self.assertAlmostEqual(actual, wanted)

    def test_worker_loads_numpy_before_graph_layout(self):
        from _manim_namespace import required_packages

        self.assertEqual(required_packages("Graph(['a'], [])"), ("numpy",))
        self.assertEqual(required_packages("DiGraph(['a'], [])"), ("numpy",))
        self.assertEqual(required_packages("Circle()"), ())

    def test_spring_seed_none_uses_the_shared_numpy_random_stream(self):
        import numpy as np

        previous_state = np.random.get_state()
        try:
            np.random.seed(123)
            expected = np.random.rand(2, 2)
            expected_next = np.random.rand()
            np.random.seed(123)
            result = graph._spring_layout_config(None, (0, 0), {"a": 0, "b": 1}, 2.0)
            actual_next = np.random.rand()
        finally:
            np.random.set_state(previous_state)
        actual = result[1]
        self.assertEqual(actual, [tuple(point) for point in expected])
        self.assertEqual(actual_next, expected_next)

    def test_spring_config_supports_fixed_positions_and_unscaled_layout(self):
        value = graph.Graph(
            ["a", "b", "c"], [("a", "b")],
            layout_config={
                "seed": 7,
                "pos": {"a": (1, 0), "b": (2, 0), "c": (2, 2)},
                "fixed": ["a"],
                "center": (3, -2),
                "iterations": 12,
                "threshold": 0,
            },
            layout_scale=None,
        )
        call = self.context.creation_calls[0]
        self.assertEqual(call[4:7], ([0], True, 1.0))
        self.assertEqual(call[7:9], (3.0, -2.0))
        self.assertAlmostEqual(call[9], 2 / (3 ** 0.5))
        self.assertEqual(call[10:13], (12, 0.0, False))
        self.assertEqual(call[3][:2], [1.0, 0.0])
        self.assertIsNotNone(value._graph_handle)

    def test_spring_change_layout_routes_positions_and_options_to_rust(self):
        value = graph.Graph({"a": (0, 0), "b": (1, 0)}, [("a", "b")])
        value.change_layout(
            "spring", scale=None,
            layout_config={"seed": 4, "pos": {"a": (2, 1)}, "fixed": []},
        )
        call = self.context.spring_layout_calls[0]
        self.assertIs(call[0], value._graph_handle)
        self.assertEqual(call[2:5], ([], True, 1.0))
        self.assertEqual(call[5:7], (0.0, 0.0))
        self.assertAlmostEqual(call[7], 2 / (2 ** 0.5))
        self.assertEqual(call[8:11], (50, 1e-4, False))

    def test_spring_fixed_nodes_outside_the_graph_are_ignored_by_networkx(self):
        graph.Graph(
            ["a", "b"], [],
            layout_config={
                "pos": {"a": (1, 0), "b": (0, 1), "ignored": (4, 0)},
                "fixed": ["ignored"],
            },
        )
        call = self.context.creation_calls[0]
        self.assertEqual(call[4:6], ([], True))
        self.assertAlmostEqual(call[9], 4 / (2 ** 0.5))
        self.assertFalse(call[12])

    def test_spring_layout_rejects_unsupported_configuration_before_publication(self):
        invalid = [
            ({"dim": 3}, NotImplementedError, "two-dimensional"),
            ({"scale": 1}, NotImplementedError, "unsupported spring"),
            ({"fixed": ["a"]}, ValueError, "require a pos mapping"),
            ({"pos": {"a": (0, 0)}, "fixed": ["missing"]}, ValueError, "missing from pos"),
            ({"k": 0}, ValueError, "non-zero"),
            ({"iterations": 2.5}, TypeError, "must be an integer"),
            ({"weight": 1}, TypeError, "string or None"),
        ]
        for config, error, message in invalid:
            with self.subTest(config=config):
                with self.assertRaisesRegex(error, message):
                    graph.Graph(["a", "b"], [], layout_config=config)
        self.assertEqual(self.context.creation_calls, [])

    def test_copy_reconstructs_an_independent_native_handle(self):
        original = graph.Graph({"a": (0, 0), "b": (1, 0)}, [("a", "b")])
        copied = original.copy()
        self.assertIsNot(copied._graph_handle, original._graph_handle)
        self.assertIsNot(copied._semantic_family_handle, original._semantic_family_handle)
        self.assertEqual(copied._graph_key_ids, original._graph_key_ids)

    def test_failed_publication_does_not_change_key_dictionary(self):
        value = graph.Graph({"a": (0, 0), "b": (1, 0)}, [("a", "b")])
        before = dict(value._graph_key_ids)
        self.context.fail = "remove_vertices"
        with self.assertRaisesRegex(RuntimeError, "remove_vertices"):
            value.remove_vertices(["a"])
        self.assertEqual(value._graph_key_ids, before)
        self.context.fail = "add_vertices"
        with self.assertRaisesRegex(RuntimeError, "add_vertices"):
            value.add_vertices({"c": (2, 0)})
        self.assertEqual(value._graph_key_ids, before)

    def test_missing_add_edge_endpoint_is_rejected_before_publication(self):
        value = graph.Graph({"a": (0, 0)}, [])
        with self.assertRaisesRegex(ValueError, "unknown vertex"):
            value.add_edges([("a", "missing")])

    def test_list_vertices_and_explicit_mapping_use_one_constructor(self):
        value = graph.Graph(["a", "b"], [("a", "b")], layout={"a": (0, 0), "b": (1, 2)})
        self.assertEqual(value._graph_key_ids, {"a": 0, "b": 1})
        self.assertEqual(self.context.creation_calls[0][:-1], (False, [0, 1], [0.0, 0.0, 1.0, 2.0], [0, 1]))

    def test_circular_layout_config_center_and_dimension_reach_rust(self):
        value = graph.Graph(
            ["left", "right"], [("left", "right")], layout="circular",
            layout_scale=1.4, layout_config={"center": (3, -1), "dim": 2},
        )
        self.assertEqual(
            self.context.creation_calls[0][:-1],
            (False, [0, 1], [0, 1], 1.4, 3.0, -1.0),
        )

        value.change_layout(
            "circular", scale=1.7, center=(0, 0),
            layout_config={"center": (2, 4), "dim": 2},
        )
        self.assertEqual(
            self.context.layout_calls[0][1:],
            (1.7, 2.0, 4.0),
        )

    def test_circular_layout_config_rejects_unqualified_dimensions_and_options(self):
        for layout_config, error, message in [
            ({"dim": 1}, ValueError, "at least 2"),
            ({"dim": 3}, NotImplementedError, "two-dimensional"),
            ({"seed": 1}, NotImplementedError, "unsupported circular layout_config"),
        ]:
            with self.subTest(layout_config=layout_config):
                with self.assertRaisesRegex(error, message):
                    graph.Graph(["a", "b"], [], layout="circular",
                                layout_config=layout_config)
        self.assertEqual(self.context.creation_calls, [])

    def test_shell_layout_config_reaches_rust_for_construction_and_live_change(self):
        value = graph.Graph(
            ["a", "b", "c"], [("a", "b"), ("b", "c")], layout="shell",
            layout_scale=2.5,
            layout_config={
                "nlist": [["b"], ["a", "c"]],
                "rotate": 0.25,
                "center": (2, -1),
                "dim": 2,
            },
        )
        self.assertEqual(
            self.context.creation_calls[0][:-1],
            (False, [0, 1, 2], [0, 1, 1, 2], [1, 0, 2], [1, 2], 2.5, 2.0, -1.0, 0.25),
        )
        value.change_layout(
            "shell", scale=1.5,
            layout_config={"nlist": [["c", "a"], ["b"]], "center": (-2, 3)},
        )
        self.assertEqual(
            self.context.shell_layout_calls[0][1:],
            ([2, 0, 1], [2, 1], 1.5, -2.0, 3.0, None),
        )

        value.remove_vertices(["b"])
        value.change_layout("shell", layout_config={"nlist": [["c", "a"]]})
        self.assertEqual(
            self.context.shell_layout_calls[1][1:],
            ([1, 0], [2], 2.0, 0.0, 0.0, None),
        )

    def test_shell_layout_rejects_unqualified_or_incomplete_options_before_publication(self):
        invalid = [
            ({"dim": 3}, ValueError, "only dim=2"),
            ({"unknown": True}, NotImplementedError, "unsupported shell layout_config"),
            ({"nlist": [["a", "a"], ["b"]]}, ValueError, "exactly once"),
            ({"nlist": [["a"]]}, ValueError, "exactly once"),
            ({"nlist": []}, ValueError, "must not be empty"),
        ]
        for config, error, message in invalid:
            with self.subTest(config=config):
                with self.assertRaisesRegex(error, message):
                    graph.Graph(["a", "b"], [], layout="shell", layout_config=config)
        self.assertEqual(self.context.creation_calls, [])

    def test_shell_singleton_uses_center_without_evaluating_shell_parameters(self):
        graph.Graph(
            ["solo"], [], layout="shell",
            layout_config={"nlist": [["missing"]], "rotate": float("nan")},
        )
        self.assertEqual(
            self.context.creation_calls[0][:-1],
            (False, [0], [], [], [], 2.0, 0.0, 0.0, None),
        )

    def test_spiral_layout_config_reaches_rust_for_construction_and_live_change(self):
        value = graph.Graph(
            ["a", "b", "c"], [("a", "b"), ("b", "c")], layout="spiral",
            layout_scale=1.5,
            layout_config={
                "resolution": 0.2,
                "equidistant": True,
                "center": (2, -1),
                "dim": 2,
            },
        )
        self.assertEqual(
            self.context.creation_calls[0][:-1],
            (False, [0, 1, 2], [0, 1, 1, 2], 1.5, 2.0, -1.0, 0.2, True),
        )
        value.change_layout(
            "spiral", scale=0.75,
            layout_config={"resolution": 0.4, "center": (-2, 3)},
        )
        self.assertEqual(
            self.context.spiral_layout_calls[0][1:],
            (0.75, -2.0, 3.0, 0.4, False),
        )

    def test_spiral_layout_rejects_unqualified_options_and_dimensions(self):
        for config, error, message in [
            ({"dim": 3}, ValueError, "only dim=2"),
            ({"unknown": True}, NotImplementedError, "unsupported spiral layout_config"),
            ({"resolution": float("nan")}, ValueError, "finite"),
            ({"equidistant": 1}, TypeError, "must be a bool"),
        ]:
            with self.subTest(config=config):
                with self.assertRaisesRegex(error, message):
                    graph.Graph(["a", "b"], [], layout="spiral", layout_config=config)
        self.assertEqual(self.context.creation_calls, [])

    def test_spiral_singleton_uses_center_without_evaluating_spiral_parameters(self):
        graph.Graph(
            ["solo"], [], layout="spiral",
            layout_config={"center": (3, 4), "resolution": float("nan"), "equidistant": 1},
        )
        self.assertEqual(
            self.context.creation_calls[0][:-1],
            (False, [0], [], 2.0, 3.0, 4.0, 0.35, False),
        )

    def test_partite_layout_preserves_partition_order_missing_nodes_and_alignment(self):
        value = graph.Graph(
            ["a", "b", "c", "d"], [("a", "b")], layout="partite",
            layout_scale=1.2,
            layout_config={
                "partitions": [["c", "a"], ["b"]],
                "align": "horizontal",
                "center": (2, -1),
            },
        )
        self.assertEqual(
            self.context.creation_calls[0][:-1],
            (False, [0, 1, 2, 3], [0, 1], [0, 2, 1, 3], [2, 1, 1],
             1.2, 2.0, -1.0, True),
        )
        value.remove_vertices(["b"])
        value.change_layout(
            "partite", scale=0.8,
            layout_config={"partitions": [["c"], ["a"]]},
        )
        self.assertEqual(
            self.context.partite_layout_calls[0][1:],
            ([1, 0, 2], [1, 1, 1], 0.8, 0.0, 0.0, False),
        )

        digraph = graph.DiGraph(
            ["a", "b"], [("a", "b")], layout="partite",
            layout_config={"partitions": [["a"], ["b"]]},
        )
        self.assertTrue(self.context.creation_calls[1][0])

    def test_partite_layout_rejects_missing_or_unsupported_configuration(self):
        invalid = [
            ({}, ValueError, "requires a non-empty partitions"),
            ({"partitions": []}, ValueError, "requires a non-empty partitions"),
            ({"partitions": [["missing"]]}, ValueError, "unknown vertex"),
            ({"partitions": [["a"]], "align": "diagonal"}, ValueError, "align must be"),
            ({"partitions": [["a"]], "dim": 2}, NotImplementedError, "unsupported partite"),
        ]
        for config, error, message in invalid:
            with self.subTest(config=config):
                with self.assertRaisesRegex(error, message):
                    graph.Graph(["a", "b"], [], layout="partite", layout_config=config)
        self.assertEqual(self.context.creation_calls, [])

    def test_global_style_configs_are_coerced_into_one_typed_rust_options_handle(self):
        graph.DiGraph(
            ["a", "b"], [("a", "b")],
            vertex_config={
                "color": "#112233",
                "fill_color": "#445566",
                "stroke_color": "#778899",
                "fill_opacity": 0.4,
                "stroke_width": 6,
                "radius": 0.2,
            },
            edge_config={"stroke_color": "#ABCDEF", "stroke_width": 3},
        )
        options = self.context.options[0]
        self.assertEqual(
            options.calls,
            [
                ("fill", (17 / 255, 34 / 255, 51 / 255, 1.0)),
                ("stroke", (17 / 255, 34 / 255, 51 / 255, 1.0)),
                ("fill", (68 / 255, 85 / 255, 102 / 255, 1.0)),
                ("stroke", (119 / 255, 136 / 255, 153 / 255, 1.0)),
                ("radius", (0.2,)),
                ("fill_opacity", (0.4,)),
                ("vertex_width", (6.0,)),
                ("edge_color", (171 / 255, 205 / 255, 239 / 255, 1.0)),
                ("edge_width", (3.0,)),
            ],
        )

    def test_per_key_vertex_and_edge_style_configs_apply_after_publication(self):
        with patch.object(graph, "_apply_key_style") as apply_style:
            graph.Graph(
                ["a", "b"], [("a", "b")],
                vertex_config={"a": {"fill_color": "#112233"}},
                edge_config={("a", "b"): {"color": "#445566", "stroke_width": 2}},
            )
        self.assertEqual(apply_style.call_count, 2)
        vertex_style = apply_style.call_args_list[0].args[1]
        edge_style = apply_style.call_args_list[1].args[1]
        self.assertEqual(vertex_style["fill_color"], graph._compat._as_color("color", "#112233"))
        self.assertEqual(edge_style["color"], graph._compat._as_color("color", "#445566"))
        self.assertEqual(edge_style["stroke_width"], 2)
        self.assertEqual(graph._compat._manim_stroke_width(edge_style["stroke_width"]), 0.02)
        self.assertEqual(len(self.context.creation_calls), 1)

    def test_invalid_per_key_and_unsupported_style_configs_fail_before_publication(self):
        with self.assertRaisesRegex(NotImplementedError, "per-key vertex_config key 0"):
            graph.Graph(["a"], [], vertex_config={0: {"fill_color": "#ffffff"}})
        with self.assertRaisesRegex(NotImplementedError, "unsupported per-key vertex_config"):
            graph.Graph(["a"], [], vertex_config={"a": {"radius": 0.2}})
        with self.assertRaisesRegex(NotImplementedError, "per-key vertex_config"):
            graph.Graph(["a"], [], vertex_config={0: "#ffffff"})
        with self.assertRaisesRegex(TypeError, "unsupported edge_config"):
            graph.Graph(["a"], [], edge_config={"radius": 0.2})
        self.assertEqual(self.context.creation_calls, [])

    def test_invalid_layout_is_rejected_before_allocating_graph(self):
        for layout in ["unsupported", {"a": (0, 0)}]:
            with self.subTest(layout=layout):
                with self.assertRaises((NotImplementedError, ValueError)):
                    graph.Graph(["a", "b"], [("a", "b")], layout=layout)
        with self.assertRaises(ValueError):
            graph.Graph(["a"], [], layout_scale=float("nan"))
        self.assertEqual(self.context.creation_calls, [])

    def test_vertex_access_reuses_retained_identity_and_rejects_unknown_keys(self):
        value = graph.Graph(["a"], [])
        handle = SimpleNamespace(semanticSlot=7, semanticGeneration=2)
        value._graph_handle.vertex = lambda key: handle if key == 0 else None
        first = value["a"]
        self.assertIs(value["a"], first)
        self.assertIs(value.vertices["a"], first)
        self.assertIs(first._semantic_handle, handle)
        self.assertIs(first._canonical_live_target_context, self.context)
        with self.assertRaises(KeyError):
            value["missing"]
