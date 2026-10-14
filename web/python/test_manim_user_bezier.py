import unittest

try:
    import numpy as np
except ImportError:
    np = None

import _manim_namespace as namespace
import _manim_user_vectors as helpers
import noon


class UserBezier(unittest.TestCase):
    def test_public_callable_and_numpy_discovery(self):
        self.assertIs(noon.bezier, helpers.bezier)
        self.assertIn("bezier", noon.__all__)
        self.assertEqual(namespace.required_packages("bezier(points)(0.5)"), ("numpy",))

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_original_array_borrow_and_list_copy_are_preserved(self):
        array = np.array([[1., 2.], [3., 4.]])
        points = array.tolist()
        borrowed = noon.bezier(array)
        copied = noon.bezier(points)
        array[0, 0] = 9
        points[0][0] = 9
        np.testing.assert_array_equal(borrowed(.5), [6, 3])
        np.testing.assert_array_equal(copied(.5), [2, 3])
        value = borrowed(.5)
        value[:] = -1
        np.testing.assert_array_equal(borrowed(.5), [6, 3])

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_scalar_and_column_parameters_preserve_polynomial_values(self):
        points = np.array([[0., 0.], [1., 2.], [2., 0.], [3., 2.]])
        curve = noon.bezier(points)
        np.testing.assert_array_equal(curve(.5), [1.5, 1])
        np.testing.assert_array_equal(curve(np.array([[0.], [.5], [1.]])),
                                      [[0, 0], [1.5, 1], [3, 2]])
        points.flags.writeable = False
        np.testing.assert_array_equal(curve(0), points[0])

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_high_degree_uses_original_float64_work_buffer(self):
        points = np.arange(15, dtype=np.float32).reshape(5, 3)
        before = points.copy()
        result = noon.bezier(points)(np.float32(.5))
        self.assertEqual(result.dtype, np.dtype("float64"))
        np.testing.assert_array_equal(result, [6, 7, 8])
        np.testing.assert_array_equal(points, before)
        self.assertFalse(np.shares_memory(result, points))


if __name__ == "__main__":
    unittest.main()
