from decimal import Decimal
import unittest
import _manim_rate_functions as rates
import noon

class BackEasings(unittest.TestCase):
    def test_module_identity_interval_guards_and_overshoot(self):
        for name in ('ease_in_back', 'ease_out_back', 'ease_in_out_back'):
            function = getattr(noon.rate_functions, name)
            self.assertIs(function, getattr(rates, name))
            self.assertEqual(rates.easing_from_rate_func(function), name)
            self.assertEqual(function(-1), 0)
            self.assertEqual(function(2), 1)
            self.assertEqual(function(float('nan')), 1)
        self.assertLess(rates.ease_in_back(.25), 0)
        self.assertGreater(rates.ease_out_back(.75), 1)
        self.assertAlmostEqual(rates.ease_in_out_back(.25), 1 - rates.ease_in_out_back(.75))

    def test_original_float_constants_preserve_decimal_error(self):
        for function in (rates.ease_in_back, rates.ease_out_back, rates.ease_in_out_back):
            with self.assertRaises(TypeError):
                function(Decimal('.25'))

if __name__ == '__main__': unittest.main()
