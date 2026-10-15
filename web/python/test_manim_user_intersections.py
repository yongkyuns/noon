import unittest
try:
    import numpy as np
except ImportError:
    np = None
import _manim_namespace as namespace
import _manim_user_vectors as helpers
import noon


class UserIntersections(unittest.TestCase):
    def test_public_identities_and_optional_package_discovery(self):
        for name in ('line_intersection', 'find_intersection'):
            self.assertIs(getattr(noon, name), getattr(helpers, name))
            self.assertIn(name, noon.__all__)
            self.assertEqual(namespace.required_packages(f'from noon import {name} as intersect\nintersect([], [])'), ('numpy',))

    def test_module_helpers_are_not_top_level_exports(self):
        for name in ('normalize_along_axis', 'rotation_matrix_from_quaternion', 'rotation_matrix_transpose_from_quaternion'):
            self.assertTrue(callable(getattr(helpers, name)))
            self.assertNotIn(name, noon.__all__)
            self.assertFalse(hasattr(noon, name))

    @unittest.skipIf(np is None, 'NumPy contracts run in the pinned utility lane')
    def test_normalization_preserves_identity_views_and_zero_rows(self):
        owner = np.array([[3., 4., 0., 99.], [0., 0., 0., 77.]])
        view = owner[:, :3]
        self.assertIs(helpers.normalize_along_axis(view, 1), view)
        np.testing.assert_allclose(owner, [[0.6, 0.8, 0, 99], [0, 0, 0, 77]])
        integer = np.array([[3, 4]])
        with self.assertRaises(TypeError):
            helpers.normalize_along_axis(integer, 1)
        np.testing.assert_array_equal(integer, [[3, 4]])

    @unittest.skipIf(np is None, 'NumPy contracts run in the pinned utility lane')
    def test_quaternion_matrix_orientation_and_input_preservation(self):
        quaternion = np.array([np.sqrt(0.5), 0, 0, np.sqrt(0.5)])
        before = quaternion.copy()
        matrix = helpers.rotation_matrix_from_quaternion(quaternion)
        transpose = helpers.rotation_matrix_transpose_from_quaternion(quaternion)
        self.assertIsInstance(transpose, list)
        np.testing.assert_allclose(matrix, [[0, -1, 0], [1, 0, 0], [0, 0, 1]], atol=1e-15)
        np.testing.assert_array_equal(matrix.T, transpose)
        np.testing.assert_array_equal(quaternion, before)

    @unittest.skipIf(np is None, 'NumPy contracts run in the pinned utility lane')
    def test_planar_intersection_and_original_rejections(self):
        first = np.array([[0, 0, 0], [2, 2, 0]], dtype=np.float32)
        second = np.array([[0, 2, 0], [2, 0, 0]], dtype=np.float32)
        np.testing.assert_array_equal(noon.line_intersection(first, second), [1, 1, 0])
        with self.assertRaisesRegex(ValueError, 'parallel'):
            noon.line_intersection(first, first)
        with self.assertRaisesRegex(TypeError, r'cross\(\) takes 2 positional arguments but 3 were given'):
            noon.line_intersection(np.zeros((3, 3)), np.zeros((3, 3)))
        self.assertNotIn('cross', noon.__all__)
        second[0, 2] = 1
        with self.assertRaisesRegex(ValueError, 'xy-plane'):
            noon.line_intersection(first, second)

    @unittest.skipIf(np is None, 'NumPy contracts run in the pinned utility lane')
    def test_batched_closest_points_and_strict_zip_are_host_data_only(self):
        origin = np.array([[0., 0., 0.]])
        x_axis = np.array([[1., 0., 0.]])
        second = np.array([[1., 2., 3.]])
        y_axis = np.array([[0., 1., 0.]])
        result = noon.find_intersection(origin, x_axis, second, y_axis)
        self.assertIsInstance(result, list)
        np.testing.assert_array_equal(result[0], [1, 0, 0])
        np.testing.assert_array_equal(origin, [[0, 0, 0]])
        with self.assertRaises(ValueError):
            noon.find_intersection(origin, x_axis, np.repeat(second, 2, axis=0), y_axis)


if __name__ == '__main__':
    unittest.main()
