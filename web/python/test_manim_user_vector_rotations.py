"""Explicit user rotation data stays separate from semantic geometry."""
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


class UserVectorRotationPackages(unittest.TestCase):
    def test_scipy_preparation_covers_direct_alias_and_module_calls(self):
        for name in ("rotate_vector", "compass_directions"):
            for source in (f"{name}(value)",
                           f"from noon import {name} as helper\nhelper(value)",
                           f"import noon\nnoon.{name}(value)"):
                with self.subTest(source=source):
                    self.assertEqual(namespace.required_packages(source), ("scipy",))

    def test_public_helpers_import_without_numeric_packages(self):
        source = """import builtins
original=builtins.__import__
def without_numeric(name,*args,**kwargs):
    if name.split('.')[0] in {'numpy','scipy'}:raise AssertionError('eager numeric import')
    return original(name,*args,**kwargs)
builtins.__import__=without_numeric
import noon
import _manim_user_vectors as helpers
assert noon.rotate_vector is helpers.rotate_vector
assert noon.compass_directions is helpers.compass_directions
"""
        result = subprocess.run([sys.executable, "-S", "-c", source],
                                cwd=Path(__file__).parent, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


@unittest.skipIf(Rotation is None, "SciPy profiles run in the pinned utility lane")
class UserVectorRotationData(unittest.TestCase):
    def test_planar_inputs_produce_three_components_without_mutating_axis(self):
        axis = np.array([0., 0., 2.])
        axis.flags.writeable = False
        result = noon.rotate_vector([1, 0], np.pi / 2, axis)
        np.testing.assert_allclose(result, [0, 1, 0], atol=1e-15)
        np.testing.assert_array_equal(axis, [0, 0, 2])
        self.assertFalse(np.shares_memory(axis, result))

    def test_compass_preserves_cardinal_values_and_original_count_errors(self):
        result = noon.compass_directions()
        np.testing.assert_allclose(result, [[1, 0, 0], [0, 1, 0], [-1, 0, 0], [0, -1, 0]], atol=1e-15)
        with self.assertRaises(ZeroDivisionError):
            noon.compass_directions(0)
        with self.assertRaises(TypeError):
            noon.compass_directions(2.0)
        with self.assertRaises(ValueError):
            noon.rotate_vector([1, 2, 3, 4], .2)


if __name__ == "__main__":
    unittest.main()
