"""Annotation constructor call shape and shared paint argument precedence."""
import inspect
import unittest
from unittest.mock import patch

import noon
from _manim_geometry import AnnotationDot, Dot, DEFAULT_DOT_RADIUS


class AnnotationDotTests(unittest.TestCase):
    def test_defaults_and_public_export(self):
        self.assertIs(noon.AnnotationDot, AnnotationDot)
        parameters = inspect.signature(AnnotationDot).parameters
        self.assertEqual(parameters["radius"].default, DEFAULT_DOT_RADIUS * 1.3)
        self.assertEqual(parameters["stroke_width"].default, 5)
        with patch.object(Dot, "__init__") as construct:
            AnnotationDot()
            construct.assert_called_once_with(radius=0.08 * 1.3, stroke_width=5,
                                              stroke_color=noon.WHITE, fill_color=noon.BLUE)

    def test_explicit_options_forward_without_a_second_geometry_or_style(self):
        point = object()
        with patch.object(Dot, "__init__") as construct:
            AnnotationDot(0.2, 7, noon.RED, noon.GREEN, point=point,
                          color=noon.BLUE, fill_opacity=0.3)
            construct.assert_called_once_with(radius=0.2, stroke_width=7,
                                              stroke_color=noon.RED, fill_color=noon.GREEN,
                                              point=point, color=noon.BLUE, fill_opacity=0.3)


if __name__ == "__main__":
    unittest.main()
