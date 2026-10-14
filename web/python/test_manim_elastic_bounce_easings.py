import unittest
import noon
import _manim_rate_functions as rates

NAMES = ('ease_in_elastic', 'ease_out_elastic', 'ease_in_out_elastic', 'ease_in_bounce', 'ease_out_bounce', 'ease_in_out_bounce')

class ElasticBounceEasings(unittest.TestCase):
    def test_module_identity_guards_and_exact_endpoints(self):
        for name in NAMES:
            f = getattr(noon.rate_functions, name)
            self.assertIs(f, getattr(rates, name))
            self.assertEqual(rates.easing_from_rate_func(f), name)
            for value, expected in ((-1, 0), (0, 0), (1, 1), (2, 1), (float('nan'), 1)):
                self.assertEqual(f(value), expected)

    def test_overshoot_and_piecewise_bounce_reversals(self):
        self.assertLess(rates.ease_in_elastic(.25), 0)
        self.assertGreater(rates.ease_out_elastic(.125), 1)
        for t in (0, .125, .25, .5, .75, .875, 1):
            self.assertAlmostEqual(rates.ease_in_bounce(t), 1-rates.ease_out_bounce(1-t))
        for t in (1/2.75, 2/2.75, 2.5/2.75):
            self.assertAlmostEqual(rates.ease_out_bounce(t), 1)

if __name__ == '__main__': unittest.main()
