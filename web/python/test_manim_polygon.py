import sys
import types
import unittest
from math import gcd, pi
from unittest.mock import patch

import _manim_polygon as polygon


class PolygonAdapters(unittest.TestCase):
    def test_regular_defaults_and_star_options_delegate_without_python_geometry(self):
        calls = []

        def regular(*args):
            calls.append(("regular", args))
            count = args[0] // gcd(args[0], args[1])
            start = args[3]
            resolved = start if start is not None else (0.0 if count % 2 == 0 else pi / 2)
            return types.SimpleNamespace(resolvedStartAngle=resolved)

        def star(*args):
            calls.append(("star", args))
            start = args[4]
            resolved = start if start is not None else (0.0 if args[0] % 2 == 0 else pi / 2)
            return types.SimpleNamespace(resolvedStartAngle=resolved)

        options = types.SimpleNamespace(
            regularPolygram=regular,
            star=star,
        )
        with patch.object(polygon._shared, "_create_geometry_handle", object()), \
             patch.object(polygon._shared, "_geometry_options", options), \
             patch.object(polygon._shared, "_attach_geometry_options"), \
             patch.object(polygon._shared, "_apply_shared_constructor_options"):
            regular_default = polygon.RegularPolygon()
            star_default = polygon.Star()
            regular_explicit = polygon.RegularPolygram(
                6, density=2, radius=2, start_angle=0.3
            )
        self.assertEqual(calls[0], ("regular", (6, 1, 1.0, None)))
        self.assertEqual(calls[1], ("star", (5, 1.0, None, 2, polygon._base.TAU / 4)))
        self.assertEqual(calls[2], ("regular", (6, 2, 2.0, 0.3)))
        self.assertEqual(regular_default.start_angle, 0.0)
        self.assertEqual(star_default.start_angle, polygon._base.TAU / 4)
        self.assertEqual(regular_explicit.start_angle, 0.3)

    def test_regular_shapes_retain_resolved_rust_start_angle(self):
        def regular(*args):
            count = args[0] // gcd(args[0], args[1])
            start = args[3]
            resolved = (
                start if start is not None else (0.0 if count % 2 == 0 else pi / 2)
            )
            return types.SimpleNamespace(resolvedStartAngle=resolved)

        def star(*args):
            start = args[4]
            resolved = (
                start if start is not None else (0.0 if args[0] % 2 == 0 else pi / 2)
            )
            return types.SimpleNamespace(resolvedStartAngle=resolved)

        options = types.SimpleNamespace(regularPolygram=regular, star=star)
        with patch.object(polygon._shared, "_create_geometry_handle", object()), \
             patch.object(polygon._shared, "_geometry_options", options), \
             patch.object(polygon._shared, "_attach_geometry_options"), \
             patch.object(polygon._shared, "_apply_shared_constructor_options"):
            even = polygon.RegularPolygon()
            compound = polygon.RegularPolygram(6, density=2)
            explicit = polygon.RegularPolygram(5, start_angle=0.3)
            star_default = polygon.Star()
            star_none = polygon.Star(6, start_angle=None)

        self.assertEqual(even.start_angle, 0.0)
        self.assertEqual(compound.start_angle, pi / 2)
        self.assertEqual(explicit.start_angle, 0.3)
        self.assertEqual(star_default.start_angle, polygon._base.TAU / 4)
        self.assertEqual(star_none.start_angle, 0.0)

    def test_triangle_inherits_polygon_queries_and_delegates_regular_options(self):
        from _manim_geometry import Triangle
        calls = []
        options = types.SimpleNamespace(
            regularPolygram=lambda *args: calls.append(args) or types.SimpleNamespace(
                resolvedStartAngle=args[3] if args[3] is not None else pi / 2
            ),
        )
        with patch.object(polygon._shared, "_create_geometry_handle", object()), \
             patch.object(polygon._shared, "_geometry_options", options), \
             patch.object(polygon._shared, "_attach_geometry_options"), \
             patch.object(polygon._shared, "_apply_shared_constructor_options"):
            default = Triangle()
            explicit = Triangle(radius=2, start_angle=0.3)
        self.assertEqual(calls, [(3, 1, 1.0, None), (3, 1, 2.0, 0.3)])
        self.assertIsInstance(default, polygon.RegularPolygon)
        self.assertEqual(default.start_angle, pi / 2)
        self.assertEqual(explicit.start_angle, 0.3)
        self.assertIs(Triangle.get_vertices, polygon.Polygram.get_vertices)
        self.assertIs(Triangle.get_vertex_groups, polygon.Polygram.get_vertex_groups)

    def test_polygon_only_coerces_vertices_and_preserves_group_lengths(self):
        calls = []
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = lambda values: values
        options = types.SimpleNamespace(polygram=lambda *args: calls.append(args) or object())
        with patch.dict(sys.modules, {"pyodide.ffi": ffi}), \
             patch.object(polygon._shared, "_create_geometry_handle", object()), \
             patch.object(polygon._shared, "_geometry_options", options), \
             patch.object(polygon._shared, "_attach_geometry_options"), \
             patch.object(polygon._shared, "_apply_constructor_color"), \
             patch.object(polygon._shared, "_apply_shared_constructor_options"):
            polygon.Polygram([(0, 0), (1, 0), (0, 1)], [(2, 0), (3, 0)])
        self.assertEqual(calls, [([0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 2.0, 0.0, 3.0, 0.0], [3, 2])])

    def test_counts_do_not_wrap_at_the_wasm_boundary(self):
        for value in (-1, 2**32, True, 2.5):
            with self.assertRaises((TypeError, ValueError)):
                polygon.RegularPolygon(value)


if __name__ == "__main__":
    unittest.main()
