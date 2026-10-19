import sys
import unittest
from unittest.mock import patch

import noon
from _manim_path_editing import (
    apply_complex_function,
    apply_function,
    apply_matrix,
    apply_points_function_about_point,
)


class _Member:
    def __init__(self, points):
        self.points = points

    def apply_over_attr_arrays(self, func):
        from _manim_path_editing import _mapped_planar_points
        mapped = _mapped_planar_points(func(self.points))
        self.points = [
            [float(point[0]), float(point[1]), 0.0]
            for point in mapped
        ]
        return self


class _Root:
    def __init__(self, members):
        self.members = members
        self.critical_point_calls = []
        self.dim = 3

    def family_members_with_points(self):
        return self.members

    def get_critical_point(self, direction):
        self.critical_point_calls.append(direction)
        return noon.Vec2(1.0, -1.0)


class ManimPointFunctionTransformsTests(unittest.TestCase):
    def _python_array_mode(self):
        return patch.dict(sys.modules, {"numpy": None})

    def test_multi_mapping_uses_critical_point_default_and_selected_family_order(self):
        first = _Member([[1.0, -1.0, 0.0], [2.0, 0.0, 0.0]])
        second = _Member([[3.0, 1.0, 0.0]])
        root = _Root([second, first])
        calls = []

        def reflect(points):
            calls.append([[float(c) for c in point] for point in points])
            return [[-x, y, z] for x, y, z in points]

        with self._python_array_mode(), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(
                apply_points_function_about_point(
                    root, reflect, about_edge=(0.0, 1.0, 1.0)
                ),
                root,
            )

        self.assertEqual(root.critical_point_calls, [noon.Vec2(0.0, 1.0)])
        self.assertEqual(calls, [[[2.0, 2.0, 0.0]], [[0.0, 0.0, 0.0], [1.0, 1.0, 0.0]]])
        self.assertEqual(second.points, [[-1.0, 1.0, 0.0]])
        self.assertEqual(first.points, [[1.0, -1.0, 0.0], [0.0, 0.0, 0.0]])

    def test_apply_function_defaults_to_origin_and_maps_each_point(self):
        member = _Member([[1.0, 2.0, 0.0], [3.0, 4.0, 0.0]])
        root = _Root([member])

        with self._python_array_mode(), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(
                apply_function(
                    root,
                    lambda point: [point[0] + 2.0, point[1] - 1.0, point[2]],
                ),
                root,
            )

        self.assertEqual(root.critical_point_calls, [])
        self.assertEqual(member.points, [[3.0, 1.0, 0.0], [5.0, 3.0, 0.0]])

    def test_apply_complex_function_preserves_the_planar_rotation_profile(self):
        member = _Member([[2.0, 1.0, 0.0]])
        root = _Root([member])

        with self._python_array_mode(), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(
                apply_complex_function(root, lambda point: 1j * point, about_point=(1, 1)),
                root,
            )

        self.assertEqual(member.points, [[1.0, 2.0, 0.0]])

    def test_apply_matrix_expands_a_small_matrix_and_defaults_to_the_origin(self):
        member = _Member([[1.0, 2.0, 0.0]])
        root = _Root([member])

        with patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(apply_matrix(root, [[0.0, -1.0], [1.0, 0.0]]), root)

        self.assertEqual(root.critical_point_calls, [])
        self.assertEqual(
            [[float(component) for component in point] for point in member.points],
            [[-2.0, 1.0, 0.0]],
        )

    def test_apply_matrix_supports_explicit_pivot_without_numpy(self):
        member = _Member([[2.0, 1.0, 0.0]])
        root = _Root([member])

        with self._python_array_mode(), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(
                apply_matrix(
                    root,
                    [[0.0, -1.0], [1.0, 0.0]],
                    about_point=(1.0, 1.0, 0.0),
                ),
                root,
            )

        self.assertEqual(member.points, [[1.0, 2.0, 0.0]])

    def test_apply_matrix_rejects_spatial_results_before_mutation(self):
        member = _Member([[1.0, 2.0, 0.0]])
        root = _Root([member])
        original = [point[:] for point in member.points]

        with self._python_array_mode(), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            with self.assertRaisesRegex(NotImplementedError, "nonzero-z"):
                apply_matrix(
                    root,
                    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 1.0]],
                )

        self.assertEqual(member.points, original)

    def test_staged_callback_rejects_before_critical_or_family_queries(self):
        root = _Root([])
        with patch("_manim_updaters.canonical_callback_phase_active", return_value=True):
            with self.assertRaisesRegex(NotImplementedError, "staged callbacks"):
                apply_points_function_about_point(root, lambda points: points)
        self.assertEqual(root.critical_point_calls, [])

    def test_public_methods_are_declared_on_mobject(self):
        for name in (
            "apply_points_function_about_point",
            "apply_function",
            "apply_complex_function",
            "apply_matrix",
        ):
            with self.subTest(name=name):
                self.assertIn(name, noon.Mobject.__dict__)


if __name__ == "__main__":
    unittest.main()
