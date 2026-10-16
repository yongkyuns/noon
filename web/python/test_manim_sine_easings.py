import math
import unittest

try:
    import numpy as np
except ImportError:
    np = None

import _manim_rate_functions as rates
import _manim_namespace as namespace
import noon


class SineEasings(unittest.TestCase):
    def test_module_callable_identity_and_shared_rate_ids(self):
        for name in ('ease_in_sine', 'ease_out_sine', 'ease_in_out_sine'):
            function = getattr(noon.rate_functions, name)
            self.assertIs(function, getattr(rates, name))
            self.assertEqual(rates.easing_from_rate_func(function), name)
        self.assertEqual(namespace.required_packages('rate_functions.ease_in_sine(.5)'), ('numpy',))

    @unittest.skipIf(np is None, 'NumPy runs in the pinned easing oracle lane')
    def test_original_interval_guards_and_single_element_array_protocol(self):
        self.assertEqual(rates.ease_in_sine(-1), 0)
        self.assertEqual(rates.ease_out_sine(2), 1)
        self.assertEqual(rates.ease_in_out_sine(float('nan')), 1)
        for function in (rates.ease_in_sine, rates.ease_out_sine, rates.ease_in_out_sine):
            samples = np.array([.5])
            before = samples.copy()
            samples.flags.writeable = False
            self.assertEqual(function(samples).shape, (1,))
            np.testing.assert_array_equal(samples, before)
            with self.assertRaises(ValueError):
                function(np.array([.25, .75]))


if __name__ == '__main__':
    unittest.main()
