import unittest

try:
    import numpy as np
except ImportError:
    np = None

import _manim_namespace as namespace
import _manim_user_vectors as helpers
import noon


class UserCurveSubdivision(unittest.TestCase):
    def test_public_callables_and_numpy_discovery(self):
        for name in ("subdivide_bezier", "bezier_remap"):
            self.assertIs(getattr(noon, name), getattr(helpers, name))
            self.assertIn(name, noon.__all__)
            self.assertEqual(namespace.required_packages(f"{name}(p, 2)"), ("numpy",))

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_single_division_borrows_arrays_and_copies_lists(self):
        points = np.arange(12).reshape(4, 3)
        self.assertIs(noon.subdivide_bezier(points, 1), points)
        copied = noon.subdivide_bezier(points.tolist(), 1)
        np.testing.assert_array_equal(copied, points)
        self.assertFalse(np.shares_memory(copied, points))

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_each_subdivision_matches_its_original_parameter_interval(self):
        for count in (2, 3, 4):
            points = np.arange(count * 3, dtype=np.float32).reshape(count, 3)
            before = points.copy()
            points.flags.writeable = False
            split = noon.subdivide_bezier(points, 3).reshape(3, count, 3)
            original = noon.bezier(points)
            for index, segment_points in enumerate(split):
                segment = noon.bezier(segment_points)
                for t in (0, .5, 1):
                    np.testing.assert_allclose(segment(t), original((index + t) / 3),
                                               rtol=1e-6, atol=1e-6)
            np.testing.assert_array_equal(points, before)

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_high_degree_keeps_the_pinned_uneven_subdivision_protocol(self):
        points = np.arange(15, dtype=np.float32).reshape(5, 3)
        split = noon.subdivide_bezier(points, 3).reshape(3, 5, 3)
        np.testing.assert_array_equal(split[:, 0], [[0, 1, 2], [4, 5, 6], [6, 7, 8]])
        np.testing.assert_array_equal(split[:, -1], [[4, 5, 6], [6, 7, 8], [12, 13, 14]])

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_remapping_retains_order_and_endpoint_boundaries(self):
        curves = np.arange(24, dtype=np.float32).reshape(2, 4, 3)
        before = curves.copy()
        remapped = noon.bezier_remap(curves, 4)
        self.assertEqual(remapped.shape, (4, 4, 3))
        self.assertEqual(remapped.dtype, np.dtype("float64"))
        np.testing.assert_array_equal(remapped[0, 0], curves[0, 0])
        np.testing.assert_array_equal(remapped[1, -1], curves[0, -1])
        np.testing.assert_array_equal(remapped[2, 0], curves[1, 0])
        np.testing.assert_array_equal(remapped[3, -1], curves[1, -1])
        np.testing.assert_array_equal(curves, before)


if __name__ == "__main__":
    unittest.main()
