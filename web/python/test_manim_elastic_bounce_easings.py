import unittest
try:
    import numpy as np
except ImportError:
    np = None
import _manim_namespace as namespace
import noon
import _manim_rate_functions as rates

NAMES = ('ease_in_elastic', 'ease_out_elastic', 'ease_in_out_elastic', 'ease_in_bounce', 'ease_out_bounce', 'ease_in_out_bounce')

class ElasticBounceEasings(unittest.TestCase):
    def test_module_identity_guards_and_exact_endpoints(self):
        for name in NAMES:
            f = getattr(noon.rate_functions, name)
            self.assertIs(f, getattr(rates, name))
            self.assertEqual(rates.easing_from_rate_func(f), name)
            for value, expected in ((-1, 0), (2, 1), (float('nan'), 1)):
                self.assertEqual(f(value), expected)

        self.assertEqual(namespace.required_packages('rate_functions.ease_in_elastic(.5)'), ('numpy',))

    @unittest.skipIf(np is None, 'NumPy endpoint contracts run in the pinned utility lane')
    def test_exact_endpoints_with_original_numpy_protocol(self):
        for name in NAMES:
            f = getattr(rates, name)
            self.assertEqual(f(0), 0)
            self.assertEqual(f(1), 1)

    @unittest.skipIf(np is None, 'NumPy elastic contracts run in the pinned utility lane')
    def test_original_elastic_overshoot(self):
        self.assertLess(rates.ease_in_elastic(.25), 0)
        self.assertGreater(rates.ease_out_elastic(.125), 1)

    def test_piecewise_bounce_reversals_without_optional_numpy(self):
        for t in (0, .125, .25, .5, .75, .875, 1):
            self.assertAlmostEqual(rates.ease_in_bounce(t), 1-rates.ease_out_bounce(1-t))
        for t in (1/2.75, 2/2.75, 2.5/2.75):
            self.assertAlmostEqual(rates.ease_out_bounce(t), 1)

if __name__ == '__main__': unittest.main()
