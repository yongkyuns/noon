import unittest
from unittest.mock import patch

import numpy as np

import noon
from _manim_path_queries import (
    _reduce_point_reduction_tree,
    merged_array,
    reduce_across_dimension,
)


class ManimReduceAcrossDimensionTests(unittest.TestCase):
    def test_reduces_owned_points_then_combines_children_in_order(self):
        first = (np.array([-2.0, 1.0, -1.0, 3.0]), ())
        second = (np.array([4.0, -2.0, 5.0, 2.0]), ())
        nested = (np.array([]), (first, second))
        root = (np.array([]), (nested, first))
        calls = []

        def reduce_func(values):
            observed = [float(value) for value in values]
            calls.append(observed)
            return sum(observed)

        self.assertEqual(_reduce_point_reduction_tree(root, reduce_func, 0), 3.0)
        self.assertEqual(
            calls,
            [[-2.0, -1.0], [4.0, 5.0], [9.0, -3.0], [-2.0, -1.0], [-3.0, 6.0]],
        )

    def test_empty_and_planar_depth_profiles(self):
        empty = (np.array([]), ())
        point = (np.array([-2.0, 1.0, -1.0, 3.0]), ())
        self.assertIsNone(_reduce_point_reduction_tree(empty, max, 0))
        self.assertEqual(_reduce_point_reduction_tree(point, max, 2), 0.0)
        self.assertEqual(_reduce_point_reduction_tree(point, min, 2), 0.0)

    def test_dimension_assertions_run_before_point_queries(self):
        value = object()
        with patch("_manim_path_queries._point_reduction_tree_observation") as query:
            with self.assertRaises(AssertionError):
                reduce_across_dimension(value, max, -1)
            with self.assertRaises(AssertionError):
                reduce_across_dimension(value, max, 3)
        query.assert_not_called()

    def test_staged_callback_queries_reject_before_reading_points(self):
        value = object()
        with patch("_manim_updaters.canonical_callback_phase_active", return_value=True):
            with patch("_manim_path_queries._point_reduction_tree_observation") as query:
                with self.assertRaisesRegex(NotImplementedError, "staged callbacks"):
                    reduce_across_dimension(value, max, 0)
        query.assert_not_called()

    def test_public_method_is_declared_on_mobject(self):
        self.assertIn("reduce_across_dimension", noon.Mobject.__dict__)

    def test_get_merged_array_returns_rust_ordered_points(self):
        coordinates = np.array([10.0, 1.0, 2.0, 3.0])
        with patch("_manim_path_queries._merged_points_observation", return_value=coordinates):
            points = merged_array(object(), "points")
        np.testing.assert_array_equal(points, [[10.0, 1.0], [2.0, 3.0]])

    def test_get_merged_array_rejects_unmodeled_attributes_before_query(self):
        with patch("_manim_path_queries._merged_points_observation") as query:
            with self.assertRaisesRegex(NotImplementedError, "only the shared Rust points"):
                merged_array(object(), "rgbas")
        query.assert_not_called()

    def test_get_array_attrs_reports_the_supported_base_profile(self):
        self.assertEqual(noon.Mobject.get_array_attrs(object()), ["points"])
        self.assertIn("get_merged_array", noon.Mobject.__dict__)


if __name__ == "__main__":
    unittest.main()
