"""Protect screen constructor dispatch without emulating rectangle semantics."""
import inspect
import unittest
from unittest.mock import patch

import noon
from _manim_compat import Rectangle, ScreenRectangle


class ScreenRectangleTests(unittest.TestCase):
    def test_constructor_forwards_dimensions_and_style_to_shared_rectangle(self):
        for ratio, height in [(16 / 9, 4), (4 / 3, 3), (0.75, 2.5)]:
            with self.subTest(ratio=ratio, height=height), patch.object(Rectangle, "__init__") as construct:
                color = object()
                ScreenRectangle(ratio, height, color=color, fill_opacity=0.4)
                construct.assert_called_once_with(
                    width=ratio * height, height=height, color=color, fill_opacity=0.4
                )

    def test_default_signature_and_lazy_export(self):
        self.assertIs(noon.ScreenRectangle, ScreenRectangle)
        self.assertIn("ScreenRectangle", noon.__all__)
        parameters = inspect.signature(ScreenRectangle).parameters
        self.assertEqual(parameters["aspect_ratio"].default, 16 / 9)
        self.assertEqual(parameters["height"].default, 4)

    def test_explicit_width_rejects_before_publishing_a_rectangle(self):
        with patch.object(Rectangle, "__init__") as construct:
            with self.assertRaises(TypeError):
                ScreenRectangle(width=2)
            construct.assert_not_called()

    def test_aspect_setter_uses_observed_height_and_shared_stretch(self):
        rectangle = object.__new__(ScreenRectangle)
        with patch.object(ScreenRectangle, "height", 3.5), patch.object(
            Rectangle, "stretch_to_fit_width"
        ) as stretch:
            rectangle.aspect_ratio = 1.25
            stretch.assert_called_once_with(4.375)

    def test_aspect_getter_reads_current_shared_dimensions(self):
        rectangle = object.__new__(ScreenRectangle)
        with patch.object(ScreenRectangle, "height", 2), patch.object(ScreenRectangle, "width", 3):
            self.assertEqual(rectangle.aspect_ratio, 1.5)
        with patch.object(ScreenRectangle, "height", 4), patch.object(ScreenRectangle, "width", 3):
            self.assertEqual(rectangle.aspect_ratio, 0.75)


if __name__ == "__main__":
    unittest.main()
