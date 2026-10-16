from decimal import Decimal
from fractions import Fraction
import unittest

import _manim_rate_functions as rates
import noon


class ExponentialCircularEasings(unittest.TestCase):
    def test_module_identity_shared_ids_and_original_endpoint_guards(self):
        for family in ('expo', 'circ'):
            for direction in ('in', 'out', 'in_out'):
                name = f'ease_{direction}_{family}'
                function = getattr(noon.rate_functions, name)
                self.assertIs(function, getattr(rates, name))
                self.assertEqual(rates.easing_from_rate_func(function), name)
                self.assertEqual(function(-1), 0)
                self.assertEqual(function(0), 0)
                self.assertEqual(function(1), 1)
                self.assertEqual(function(2), 1)
                self.assertEqual(function(float('nan')), 1)

    def test_exponential_endpoint_types_and_halfway_values(self):
        self.assertIs(type(rates.ease_in_expo(0.)), int)
        self.assertIs(type(rates.ease_out_expo(1.)), int)
        self.assertEqual(rates.ease_in_expo(Fraction(1, 2)), 1 / 32)
        self.assertEqual(rates.ease_out_expo(Decimal('.5')), Decimal('0.96875'))
        self.assertEqual(rates.ease_in_out_expo(Decimal('.5')), Decimal('.5'))

    def test_circular_host_sqrt_protocol_and_symmetry(self):
        self.assertAlmostEqual(rates.ease_in_circ(.25), 0.031754163448145745)
        self.assertAlmostEqual(rates.ease_out_circ(.25), 0.6614378277661477)
        self.assertAlmostEqual(rates.ease_in_out_circ(.25), 0.0669872981077807)
        self.assertAlmostEqual(rates.ease_in_circ(.25), 1 - rates.ease_out_circ(.75))
        self.assertIs(type(rates.ease_in_circ(Decimal('.5'))), float)


if __name__ == '__main__':
    unittest.main()
