import unittest

try:
    import numpy as np
except ImportError:
    np = None

import _manim_namespace as namespace
import _manim_user_vectors as helpers
import noon


class UserCurveOperations(unittest.TestCase):
    def test_public_helpers_and_existing_interpolation_identity(self):
        for name in ("partial_bezier_points", "split_bezier", "is_closed", "straight_path"):
            self.assertIs(getattr(noon, name), getattr(helpers, name))
            self.assertIn(name, noon.__all__)
        self.assertIs(noon.straight_path(), noon.interpolate)
        self.assertEqual(namespace.required_packages("partial_bezier_points(p, 0, 1)"), ("numpy",))
        self.assertEqual(namespace.required_packages("is_closed(p); straight_path()"), ())

    def test_closed_lists_keep_the_original_sequence_error(self):
        with self.assertRaises(TypeError):
            noon.is_closed([[1, 1, 1], [1, 1, 1]])

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_signed_original_closed_tolerance_is_preserved(self):
        self.assertTrue(noon.is_closed(np.array([[1, 1, 1], [1, 1, 1]])))
        self.assertFalse(noon.is_closed(np.array([[-1, -1, -1], [-1, -1, -1]])))

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_partial_curve_matches_original_parameter_interval(self):
        points = np.array([[0., 0.], [1., 2.], [2., 0.], [3., 2.]])
        before = points.copy()
        part = noon.partial_bezier_points(points, .2, .8)
        curve = noon.bezier(points)
        segment = noon.bezier(part)
        for t in (0, .25, .5, .75, 1):
            np.testing.assert_allclose(segment(t), curve(.2 + .6 * t), rtol=1e-14, atol=1e-14)
        np.testing.assert_array_equal(points, before)
        self.assertFalse(np.shares_memory(part, points))

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_constant_partial_alias_and_endpoint_copies_follow_original(self):
        points = np.array([[2., 3., 4.]])
        self.assertIs(noon.partial_bezier_points(points, .2, .8), points)
        for a, b in ((1, .5), (.5, 0)):
            result = noon.partial_bezier_points(points, a, b)
            self.assertFalse(np.shares_memory(result, points))
            np.testing.assert_array_equal(result, points)

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_split_halves_share_boundary_and_retain_both_endpoints(self):
        for count in (2, 3, 4, 5):
            points = np.arange(count * 3, dtype=np.float32).reshape(count, 3)
            before = points.copy()
            split = noon.split_bezier(points, .25)
            self.assertEqual(split.shape, (2 * count, 3))
            np.testing.assert_array_equal(split[0], points[0])
            np.testing.assert_array_equal(split[-1], points[-1])
            np.testing.assert_array_equal(split[count - 1], split[count])
            np.testing.assert_allclose(split[count], noon.bezier(points)(.25))
            np.testing.assert_array_equal(points, before)


if __name__ == "__main__":
    unittest.main()
