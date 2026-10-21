"""Shared-path facade tests for Manim's curve-as-family wrapper."""

import unittest
from unittest.mock import PropertyMock, patch

from _manim_compat import CurvesAsSubmobjects, Group, VGroup


class CurvesAsSubmobjectsTests(unittest.TestCase):
    def test_constructor_builds_one_styled_path_object_per_cubic_tuple(self):
        curves = [object(), object(), object()]
        source = type("Source", (), {"get_cubic_bezier_tuples": lambda self: curves})()
        constructed = []

        class Part:
            def __init__(self):
                self.points = None
                self.style_source = None

            def set_points(self, points):
                self.points = points
                return self

            def match_style(self, value):
                self.style_source = value
                return self

        def init_group(value, **kwargs):
            value.init_kwargs = kwargs
            value.children = []

        def add_part(value, part):
            constructed.append(part)
            value.children.append(part)
            return value

        with (
            patch("_manim_compat.VMobject", Part),
            patch.object(VGroup, "__init__", init_group),
            patch.object(VGroup, "add", add_part),
        ):
            result = CurvesAsSubmobjects(source, color="red")

        self.assertEqual(result.init_kwargs, {"color": "red"})
        self.assertEqual([part.points for part in constructed], curves)
        self.assertTrue(all(part.style_source is source for part in constructed))
        self.assertEqual(result.children, constructed)

    def test_point_from_proportion_uses_cumulative_piece_arc_length(self):
        class Part:
            def __init__(self, name, point_count, length):
                self.name = name
                self.point_count = point_count
                self.length = length
                self.proportions = []

            def get_num_points(self):
                return self.point_count

            def get_arc_length(self):
                return self.length

            def point_from_proportion(self, alpha):
                self.proportions.append(alpha)
                return self.name, alpha

        first = Part("first", 4, 1.0)
        empty = Part("empty", 0, 100.0)
        second = Part("second", 4, 3.0)
        value = CurvesAsSubmobjects.__new__(CurvesAsSubmobjects)

        with patch.object(CurvesAsSubmobjects, "submobjects", new_callable=PropertyMock) as children:
            children.return_value = [first, empty, second]
            self.assertEqual(value.point_from_proportion(0.25), ("first", 1.0))
            self.assertEqual(value.point_from_proportion(0.5), ("second", 1.0 / 3.0))
            self.assertEqual(value.point_from_proportion(1), ("second", 1))

        self.assertEqual(first.proportions, [1.0])
        self.assertEqual(empty.proportions, [])
        self.assertEqual(second.proportions, [1.0 / 3.0, 1])

    def test_point_from_proportion_preserves_range_and_empty_family_errors(self):
        value = CurvesAsSubmobjects.__new__(CurvesAsSubmobjects)
        with patch.object(CurvesAsSubmobjects, "submobjects", new_callable=PropertyMock) as children:
            children.return_value = []
            with self.assertRaisesRegex(ValueError, "^Alpha -0.1 not between 0 and 1\\.$"):
                value.point_from_proportion(-0.1)
            with self.assertRaisesRegex(Exception, "with no submobjects$"):
                value.point_from_proportion(0.5)

            children.return_value = [type("Empty", (), {"get_num_points": lambda self: 0})()]
            with self.assertRaisesRegex(Exception, "whose submobjects have no points$"):
                value.point_from_proportion(0.5)


if __name__ == "__main__":
    unittest.main()
