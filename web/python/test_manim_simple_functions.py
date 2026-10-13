import subprocess
import inspect
from pathlib import Path
import sys
import unittest

import _manim_simple_functions as helpers
import noon
from _manim_namespace import required_packages

try:
    import numpy as np
except ImportError:
    np = None


class SimpleFunctions(unittest.TestCase):
    def test_comparison_order_and_selected_identity(self):
        calls = []
        class Value:
            def __lt__(self, other):
                calls.append(("lt", other))
                return False
            def __gt__(self, other):
                calls.append(("gt", other))
                return False
        value, lower, upper = Value(), object(), object()
        self.assertIs(noon.clip(value, lower, upper), value)
        self.assertEqual(calls, [("lt", lower), ("gt", upper)])
        self.assertEqual(noon.clip(5, 10, 0), 10)
        self.assertEqual(noon.clip("a", "h", "k"), "h")

    def test_choose_cache_errors_and_public_identity(self):
        helpers.choose.cache_clear()
        self.assertEqual(noon.choose(6, 2), 15)
        self.assertEqual(noon.choose(6, 2), 15)
        self.assertEqual(helpers.choose.cache_info().hits, 1)
        for n in range(11):
            helpers.choose(n, 0)
        self.assertEqual(helpers.choose.cache_info().currsize, 10)
        for args, error in [((-1, 0), ValueError), ((2, 1.0), TypeError), (([], 1), TypeError)]:
            with self.assertRaises(error): helpers.choose(*args)
        for name in ["choose", "clip", "binary_search", "sigmoid"]:
            self.assertIs(getattr(noon, name), getattr(helpers, name))
            self.assertIn(name, noon.__all__)
        self.assertEqual(str(inspect.signature(noon.choose)), "(n: 'int', k: 'int') -> 'int'")
        self.assertEqual(noon.clip.__annotations__,
                         {"a": "ComparableT", "min_a": "ComparableT", "max_a": "ComparableT", "return": "ComparableT"})

    def test_lazy_numpy_and_worker_alias_detection(self):
        code = "import sys; from noon import *; assert 'numpy' not in sys.modules; assert choose(4, 2) == 6; assert clip(2, 0, 1) == 1; assert 'numpy' not in sys.modules"
        subprocess.run([sys.executable, "-c", code], check=True, cwd=Path(__file__).resolve().parent)
        for name in ["sigmoid", "binary_search"]:
            self.assertEqual(required_packages(f"from noon import {name} as f\nf(0)"), ("numpy",))
            self.assertEqual(required_packages(f"import noon as n\nn.{name}(0)"), ("numpy",))

    @unittest.skipIf(np is None, "NumPy contracts run in the pinned utility lane")
    def test_search_callback_order_and_narrow_interval(self):
        calls = []
        def function(value):
            calls.append(value)
            return value
        self.assertEqual(noon.binary_search(function, 0, 0, 1), 0)
        self.assertEqual(calls, [0, 0.5, 1])
        calls.clear()
        self.assertEqual(noon.binary_search(function, 10, 0, 1, tolerance=2), 0.5)
        self.assertEqual(calls, [])
        self.assertIsNone(noon.binary_search(function, 2, 0, 1))
        self.assertAlmostEqual(noon.binary_search(lambda x: -x, -0.25, 0, 1), 0.25, delta=1e-4)

    @unittest.skipIf(np is None, "NumPy contracts run in the pinned utility lane")
    def test_sigmoid_scalar_array_and_warning_behavior(self):
        self.assertEqual(type(noon.sigmoid(0)), np.float64)
        values = np.array([-1, 0, 1], dtype=np.float32)
        result = noon.sigmoid(values)
        self.assertEqual(result.dtype, np.float32)
        np.testing.assert_array_equal(result, 1.0 / (1 + np.exp(-values)))
        with self.assertWarns(RuntimeWarning): self.assertEqual(noon.sigmoid(-1000), 0)


if __name__ == "__main__": unittest.main()
