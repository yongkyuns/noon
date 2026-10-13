import subprocess
import sys
import unittest
from pathlib import Path

try:
    import numpy as np
except ImportError:
    np = None

import _manim_namespace as namespace
import _manim_user_vectors as helpers
import noon


class UserVectors(unittest.TestCase):
    def test_public_exports_and_optional_package_discovery(self):
        optional = ["complex_to_R3", "complex_func_to_R3_func", "center_of_mass", "midpoint",
                    "normalize", "angle_of_vector", "angle_between_vectors", "shoelace",
                    "shoelace_direction"]
        for name in optional + ["cross2d", "R3_to_complex"]:
            self.assertIs(getattr(noon, name), getattr(helpers, name))
            self.assertIn(name, noon.__all__)
            expected = ("numpy",) if name in optional else ()
            self.assertEqual(namespace.required_packages(f"{name}([])"), expected)
        self.assertEqual(namespace.required_packages("from noon import normalize as unit\nunit([1, 2])"), ("numpy",))

    def test_cold_import_and_pure_complex_conversion(self):
        subprocess.run([sys.executable, "-S", "-c", "import sys; import _manim_user_vectors as h; "
                        "assert 'numpy' not in sys.modules; assert h.R3_to_complex([1, 2, 9]) == 1+2j; "
                        "f=h.complex_func_to_R3_func(lambda z:z); assert 'numpy' not in sys.modules"],
                       check=True, cwd=Path(__file__).parent)
        self.assertEqual(helpers.R3_to_complex([3]), 3 + 0j)
        with self.assertRaises(TypeError): helpers.R3_to_complex(["x", 2])

    @unittest.skipIf(np is None, "NumPy contracts run in the pinned utility lane")
    def test_normalize_preserves_original_zero_and_fallback_rules(self):
        vector = np.array([3, 4], dtype=np.float32)
        normalized = helpers.normalize(vector)
        self.assertEqual(normalized.dtype, np.float32)
        np.testing.assert_allclose(normalized, [0.6, 0.8])
        self.assertFalse(np.shares_memory(normalized, vector))
        fallback = [7, 8]
        self.assertIs(helpers.normalize([0, 0], fallback), fallback)
        with self.assertRaises(ValueError): helpers.normalize([0, 0], np.array([7, 8]))
        np.testing.assert_array_equal(helpers.normalize([], []), [])

    @unittest.skipIf(np is None, "NumPy contracts run in the pinned utility lane")
    def test_centers_batch_angles_and_cross_products(self):
        points = np.array([[1, 2, 3], [3, 4, 5]], dtype=np.int64)
        np.testing.assert_array_equal(helpers.center_of_mass(points), [2, 3, 4])
        np.testing.assert_array_equal(helpers.midpoint(points[0], points[1]), [2, 3, 4])
        np.testing.assert_allclose(helpers.angle_of_vector(np.array([[1, 0], [0, 1]])), [0, np.pi/2])
        with self.assertRaises(ValueError): helpers.angle_of_vector(np.array([[1, 2]]))
        self.assertAlmostEqual(helpers.angle_between_vectors(np.array([1, 0]), np.array([0, 1])), np.pi/2)
        np.testing.assert_array_equal(helpers.cross2d(points, np.array([[3, 4, 0], [0, 1, 0]])), [-2, 3])

    @unittest.skipIf(np is None, "NumPy contracts run in the pinned utility lane")
    def test_shoelace_observes_open_input_and_orientation(self):
        points = np.array([[0, 0], [1, 0], [1, 1], [0, 1]], dtype=np.float32)
        self.assertEqual(helpers.shoelace(points), -1)
        self.assertEqual(helpers.shoelace_direction(points), "CCW")
        self.assertEqual(helpers.shoelace_direction(points[::-1]), "CW")

    @unittest.skipIf(np is None, "NumPy contracts run in the pinned utility lane")
    def test_complex_callback_is_lazy_called_once_and_propagates_failure(self):
        calls = []
        def callback(value):
            calls.append(value)
            if value == 3: raise LookupError("user failure")
            return value * value
        converted = helpers.complex_func_to_R3_func(callback)
        self.assertEqual(calls, [])
        np.testing.assert_array_equal(converted([1, 2, 100]), [-3, 4, 0])
        self.assertEqual(calls, [1+2j])
        with self.assertRaises(LookupError): converted([3, 0, 100])
        self.assertEqual(calls, [1+2j, 3+0j])


if __name__ == "__main__":
    unittest.main()
