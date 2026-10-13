import math
import unittest

import noon
import _manim_rate_functions as rates


class RateIntervals(unittest.TestCase):
    def test_public_evaluation_uses_pinned_boundary_rules(self):
        for function in [noon.linear, noon.smooth, noon.rush_into, noon.rush_from]:
            for value in [-math.inf, -0.01]: self.assertEqual(function(value), 0)
            for value in [1.01, math.inf, math.nan]: self.assertEqual(function(value), 1)
            with self.assertRaises(TypeError): function("0.5")
        for value in [-math.inf, -0.01, 1.01, math.inf, math.nan]:
            self.assertEqual(noon.there_and_back(value), 0)
        self.assertEqual(noon.linear(0.25), 0.25)
        from fractions import Fraction
        value = Fraction(1, 3)
        self.assertIs(noon.linear(value), value)
        self.assertIs(noon.linear(True), True)
        self.assertAlmostEqual(noon.smooth(0.5), 0.5)
        self.assertAlmostEqual(noon.there_and_back(0.5), 1)

    def test_wrapping_preserves_shared_rate_identity_and_reverse_expression(self):
        for name in ["linear", "smooth", "rush_into", "rush_from", "there_and_back"]:
            function = getattr(noon, name)
            self.assertEqual(function.__name__, name)
            self.assertEqual(rates.easing_from_rate_func(function), name)
        expression = eval("lambda t: smooth(1 - t)", {"smooth": noon.smooth})
        self.assertTrue(rates.is_reverse_smooth_rate_func(expression))


if __name__ == "__main__": unittest.main()
