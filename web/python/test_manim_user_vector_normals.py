import subprocess
import sys
import unittest
from pathlib import Path

try:
    import numpy as np
except ImportError:
    np = None

import _manim_namespace as namespace
import _manim_user_vector_bases as helpers
import noon


class UserVectorNormals(unittest.TestCase):
    def test_public_identity_and_lazy_dependency(self):
        self.assertIs(noon.get_unit_normal, helpers.get_unit_normal)
        self.assertIn('get_unit_normal', noon.__all__)
        self.assertEqual(namespace.required_packages('from noon import get_unit_normal as normal\nnormal(a, b)'), ('numpy',))
        subprocess.run([sys.executable, '-S', '-c',
                        "import sys; from noon import get_unit_normal; assert 'numpy' not in sys.modules"],
                       cwd=Path(__file__).parent, check=True)

    @unittest.skipIf(np is None, 'NumPy runs in the pinned array-oracle lane')
    def test_orientation_and_original_degenerate_fallbacks(self):
        np.testing.assert_array_equal(noon.get_unit_normal([1, 0, 0], [0, 1, 0]), [0, 0, 1])
        np.testing.assert_array_equal(noon.get_unit_normal([0, 1, 0], [1, 0, 0]), [0, 0, -1])
        for first, second in [([0, 0, 0], [0, 0, 0]), ([0, 0, 0], [0, 0, 3]), ([0, 0, -2], [0, 0, 3])]:
            np.testing.assert_array_equal(noon.get_unit_normal(first, second), [0, -1, 0])
        np.testing.assert_array_equal(noon.get_unit_normal([3, 0, 0], [6, 0, 0]), [0, 0, 1])

    @unittest.skipIf(np is None, 'NumPy runs in the pinned array-oracle lane')
    def test_scaling_tolerance_and_read_only_inputs(self):
        first = np.array([1e200, 0., 0.]); second = np.array([0., 1e-200, 0.])
        first.flags.writeable = second.flags.writeable = False
        np.testing.assert_array_equal(noon.get_unit_normal(first, second), [0, 0, 1])
        np.testing.assert_array_equal(first, [1e200, 0, 0])
        np.testing.assert_array_equal(second, [0, 1e-200, 0])
        np.testing.assert_array_equal(noon.get_unit_normal([1, 0, 0], [1, 0, 1e-7], tol=1e-6), [0, 0, 1])
        np.testing.assert_allclose(noon.get_unit_normal([1, 0, 0], [1, 0, 1e-7], tol=1e-8), [0, -1, 0])
        with self.assertRaises(ValueError):
            noon.get_unit_normal([], [1, 0, 0])

    @unittest.skipIf(np is None, 'NumPy runs in the pinned array-oracle lane')
    def test_null_fallback_reuses_original_mutable_user_value(self):
        first = noon.get_unit_normal([0, 0, 0], [0, 0, 0])
        self.assertIs(first, noon.get_unit_normal([0, 0, 0], [0, 0, 1]))
        before = first.copy()
        try:
            first[0] = 7
            self.assertEqual(noon.get_unit_normal([0, 0, 0], [0, 0, 1])[0], 7)
        finally:
            first[:] = before


if __name__ == '__main__':
    unittest.main()
