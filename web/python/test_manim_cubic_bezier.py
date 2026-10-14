import unittest
from unittest.mock import patch

import _manim_compat as compat
import _manim_geometry as geometry
import noon


class CubicBezierConstructor(unittest.TestCase):
    def test_one_cubic_command_preserves_control_order_and_style_options(self):
        captured = []
        def initialize(wrapper, path, **kwargs):
            captured.append((wrapper, path.to_ir(), kwargs))
        points = [(-2, 0, 0), (-1, 2, 0), (1, -2, 0), (2, 0, 0)]
        with patch.object(compat.Path, "__init__", initialize):
            curve = noon.CubicBezier(*points, stroke_width=6, color=noon.BLUE, fill_opacity=0.2)
        self.assertIsInstance(curve, compat.VMobject)
        self.assertIs(noon.CubicBezier, geometry.CubicBezier)
        self.assertIn("CubicBezier", noon.__all__)
        self.assertEqual(len(captured), 1)
        self.assertIs(captured[0][0], curve)
        self.assertEqual(captured[0][1]["commands"], [
            {"move_to": {"to": {"x": -2, "y": 0}}},
            {"cubic_to": {"control1": {"x": -1, "y": 2},
                          "control2": {"x": 1, "y": -2}, "to": {"x": 2, "y": 0}}},
        ])
        self.assertEqual(captured[0][2], {"stroke_width": 6, "color": noon.BLUE, "fill_opacity": 0.2})

    def test_planar_rejection_precedes_shared_host_publication(self):
        points = [(0, 0, 0), (1, 1, 0), (2, 1, 0), (3, 0, 0)]
        for index in range(4):
            invalid = list(points)
            invalid[index] = (0, 0, 1)
            with patch.object(compat.Path, "__init__") as initialize:
                with self.assertRaises(NotImplementedError): noon.CubicBezier(*invalid)
                initialize.assert_not_called()

    def test_nonfinite_or_missing_controls_publish_nothing(self):
        for points in [((0, 0), (1, 1), (float("nan"), 1), (3, 0)),
                       ((0, 0), (1, 1), (2, 1)),
                       ((0, 0), (1, 1), (2, 1), None)]:
            with patch.object(compat.Path, "__init__") as initialize:
                with self.assertRaises((TypeError, ValueError)): noon.CubicBezier(*points)
                initialize.assert_not_called()


if __name__ == "__main__":
    unittest.main()
