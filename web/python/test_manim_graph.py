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
        self.family = lambda: _Family()


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

    def liveGraphCopy(self, handle):
        self._call("copy")
        copied = _Handle(self.next_token)
        self.next_token += 1
        return copied

    def liveGraphAddVertices(self, *args): self._call("add_vertices")
    def liveGraphAddEdges(self, *args): self._call("add_edges")
    def liveGraphRemoveVertices(self, *args): self._call("remove_vertices")
    def liveGraphRemoveEdges(self, *args): self._call("remove_edges")
    def liveGraphCircularLayout(self, *args): self._call("circular")
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

    def test_per_key_and_unsupported_style_configs_fail_before_graph_publication(self):
        with self.assertRaisesRegex(NotImplementedError, "per-key vertex_config"):
            graph.Graph(["a"], [], vertex_config={"a": {"fill_color": "#ffffff"}})
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
