from decimal import Decimal
from fractions import Fraction
import unittest

import _manim_rate_functions as rates
import _manim_namespace as namespace
import noon


class PolynomialEasings(unittest.TestCase):
    def test_public_callable_identity_shared_ids_and_interval_guards(self):
        for degree in ('quad', 'cubic', 'quart', 'quint'):
            for direction in ('in', 'out', 'in_out'):
                name = f'ease_{direction}_{degree}'
                function = getattr(noon, name)
                self.assertIs(function, getattr(rates, name))
                self.assertIn(name, noon.__all__)
                self.assertEqual(rates.easing_from_rate_func(function), name)
                self.assertEqual(function(-1), 0)
                self.assertEqual(function(2), 1)
                self.assertEqual(function(float('nan')), 1)
                self.assertEqual(namespace.required_packages(f'{name}(0.25)'), ())

    def test_numeric_host_types_and_operator_order(self):
        self.assertEqual(noon.ease_in_quad(Decimal('0.25')), Decimal('0.0625'))
        self.assertEqual(noon.ease_out_cubic(Fraction(1, 4)), Fraction(37, 64))
        self.assertEqual(noon.ease_in_out_quint(Fraction(1, 4)), Fraction(1, 64))
        self.assertEqual(noon.ease_in_out_quad(Decimal('0.25')), Decimal('0.125'))
        self.assertEqual(noon.ease_in_out_quad(Decimal('0.75')), Decimal('0.875'))
        self.assertEqual(noon.ease_in_out_cubic(0.25), 0.0625)
        self.assertEqual(noon.ease_in_out_cubic(0.75), 0.9375)


if __name__ == '__main__':
    unittest.main()
