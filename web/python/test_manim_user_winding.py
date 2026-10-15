import unittest

try:
    import numpy as np
except ImportError:
    np = None

import _manim_namespace as namespace
import _manim_user_vectors as helpers
import noon


class UserWinding(unittest.TestCase):
    def test_public_callable_and_optional_numpy_discovery(self):
        self.assertIs(noon.get_winding_number, helpers.get_winding_number)
        self.assertIn("get_winding_number", noon.__all__)
        self.assertEqual(namespace.required_packages(
            "from noon import get_winding_number\nget_winding_number([])"), ("numpy",))

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_orientation_offset_and_repeated_winding_preserve_input(self):
        points = np.array([[1., 1.], [-1., 1.], [-1., -1.], [1., -1.]])
        original = points.copy()
        self.assertAlmostEqual(noon.get_winding_number(points), 1)
        self.assertAlmostEqual(noon.get_winding_number(points[::-1]), -1)
        self.assertAlmostEqual(noon.get_winding_number(points + [3, 0]), 0)
        self.assertAlmostEqual(noon.get_winding_number(np.vstack([points, points])), 2)
        np.testing.assert_array_equal(points, original)
        points.flags.writeable = False
        self.assertAlmostEqual(noon.get_winding_number(points), 1)

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_empty_result_type_and_array_scalar_result_type(self):
        self.assertIs(type(noon.get_winding_number([])), float)
        self.assertIs(type(noon.get_winding_number([[1, 0]])), np.float64)
        self.assertEqual(noon.get_winding_number([]), 0)

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_unsliceable_iterator_failure_does_not_consume_user_input(self):
        points = [[1, 0], [0, 1], [-1, 0]]
        iterator = iter(points)
        with self.assertRaises(TypeError):
            noon.get_winding_number(iterator)
        self.assertEqual(list(iterator), points)


if __name__ == "__main__":
    unittest.main()
