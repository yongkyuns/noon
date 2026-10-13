import sys
import types
import unittest
from unittest.mock import patch

import _manim_polygon as polygon


class PolygonAdapters(unittest.TestCase):
    def test_regular_defaults_and_star_options_delegate_without_python_geometry(self):
        calls = []
        options = types.SimpleNamespace(
            regularPolygram=lambda *args: calls.append(("regular", args)) or object(),
            star=lambda *args: calls.append(("star", args)) or object(),
        )
        with patch.object(polygon._shared, "_create_geometry_handle", object()), \
             patch.object(polygon._shared, "_geometry_options", options), \
             patch.object(polygon._shared, "_attach_geometry_options"), \
             patch.object(polygon._shared, "_apply_shared_constructor_options"):
            polygon.RegularPolygon()
            polygon.Star()
            polygon.RegularPolygram(6, density=2, radius=2, start_angle=0.3)
        self.assertEqual(calls[0], ("regular", (6, 1, 1.0, None)))
        self.assertEqual(calls[1], ("star", (5, 1.0, None, 2, polygon._base.TAU / 4)))
        self.assertEqual(calls[2], ("regular", (6, 2, 2.0, 0.3)))

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
