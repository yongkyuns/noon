"""Regressions for explicit user-data rotation matrices, outside scene execution."""
import subprocess
import sys
import unittest
from pathlib import Path

import noon
import _manim_namespace as namespace

try:
    import numpy as np
    from scipy.spatial.transform import Rotation
except ImportError:
    np = Rotation = None


class UserRotationPackageTests(unittest.TestCase):
    def test_package_preparation_covers_direct_alias_and_module_calls(self):
        for name, package in (('rotation_matrix', 'scipy'),):
            for source in (f'{name}(angle, vector)',
                           f'from noon import {name} as helper\nhelper(angle, vector)',
                           f'import noon\nnoon.{name}(angle, vector)'):
                with self.subTest(source=source):
                    self.assertEqual(namespace.required_packages(source), (package,))
        self.assertEqual(namespace.required_packages("message = 'rotation_matrix'"), ())

    def test_lazy_public_helpers_do_not_import_numeric_packages(self):
        source = '''
import builtins
original = builtins.__import__
def strict(name, *args, **kwargs):
    if name.split('.')[0] in {'numpy', 'scipy'}:
        raise AssertionError('numeric package imported before helper evaluation')
    return original(name, *args, **kwargs)
builtins.__import__ = strict
import noon
import _manim_user_vectors as helpers
assert noon.rotation_matrix is helpers.rotation_matrix
'''
        result = subprocess.run([sys.executable, '-S', '-c', source],
                                cwd=Path(__file__).parent, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


@unittest.skipIf(Rotation is None, 'SciPy user-data contracts run in the pinned utility lane')
class UserRotationDataTests(unittest.TestCase):
    def test_homogeneous_rotation_preserves_input_and_origin(self):
        axis = np.array([0., 0., 2.])
        matrix = noon.rotation_matrix(np.pi / 2, axis, homogeneous=True)
        np.testing.assert_allclose(matrix @ [1, 0, 0, 1], [0, 1, 0, 1], atol=1e-15)
        np.testing.assert_array_equal(axis, [0, 0, 2])
        self.assertFalse(np.shares_memory(axis, matrix))

    def test_numeric_shape_and_type_errors_are_not_coerced(self):
        from decimal import Decimal
        with self.assertRaises(TypeError):
            noon.rotation_matrix(Decimal('0.2'), [0, 0, 1])
        with self.assertRaises(ValueError):
            noon.rotation_matrix(0.2, [1, 2])


if __name__ == '__main__':
    unittest.main()
