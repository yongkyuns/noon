import unittest

try:
    import numpy as np
except ImportError:
    np = None

import _manim_namespace as namespace
import _manim_user_vector_bases as helpers
import noon


class UserVectorBases(unittest.TestCase):
    def test_public_identity_and_lazy_numpy_discovery(self):
        for name in ('z_to_vector',):
            self.assertIs(getattr(noon, name), getattr(helpers, name))
            self.assertIn(name, noon.__all__)
            self.assertNotIn('cross', noon.__all__)
            self.assertNotIn('norm_squared', noon.__all__)
            self.assertEqual(namespace.required_packages(f'{name}(value)'), ('numpy',))

    @unittest.skipIf(np is None, 'NumPy runs in the pinned array-oracle lane')
    def test_cross_orientation_and_original_nonconjugate_squared_norm(self):
        np.testing.assert_array_equal(helpers.cross([1, 0, 0], [0, 1, 0]), [0, 0, 1])
        np.testing.assert_array_equal(helpers.cross([0, 1, 0], [1, 0, 0]), [0, 0, -1])
        self.assertEqual(helpers.norm_squared([1j, 2]), 3 + 0j)

    @unittest.skipIf(np is None, 'NumPy runs in the pinned array-oracle lane')
    def test_rotation_basis_degenerate_and_x_axis_profiles(self):
        np.testing.assert_array_equal(noon.z_to_vector([0, 0, 0]), np.zeros((3, 3)))
        for direction in ([1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]):
            matrix = noon.z_to_vector(direction)
            np.testing.assert_array_equal(matrix[:, 2], direction)
            np.testing.assert_array_equal(matrix.T @ matrix, np.eye(3))
            self.assertEqual(np.linalg.det(matrix), 1)

    @unittest.skipIf(np is None, 'NumPy runs in the pinned array-oracle lane')
    def test_read_only_inputs_and_errors_leave_inputs_unchanged(self):
        vector = np.array([1., 2., 3.])
        before = vector.copy()
        vector.flags.writeable = False
        noon.z_to_vector(vector)
        helpers.norm_squared(vector)
        helpers.cross(vector, vector)
        np.testing.assert_array_equal(vector, before)
        with self.assertRaises(IndexError):
            helpers.cross([1, 2], [1, 2, 3])
        with self.assertRaises(TypeError):
            noon.z_to_vector(None)


if __name__ == '__main__':
    unittest.main()
