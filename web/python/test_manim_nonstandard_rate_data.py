"""User evaluation never silently substitutes a different Rust playback curve."""
from fractions import Fraction
import math
import unittest

import noon
import _manim_rate_functions as rates

NAMES = ('there_and_back_with_pause', 'running_start', 'wiggle', 'exponential_decay')


class NonstandardRateDataTests(unittest.TestCase):
    def test_public_identity_does_not_admit_unimplemented_playback(self):
        for name in NAMES:
            with self.subTest(name=name):
                function = getattr(noon, name)
                self.assertIn(name, noon.__all__)
                self.assertIs(function, getattr(noon.rate_functions, name))
                with self.assertRaises(NotImplementedError):
                    rates.easing_from_rate_func(function)

    def test_running_start_preserves_parameter_types_and_overshoot(self):
        self.assertIsInstance(noon.running_start(Fraction(1, 4)), float)
        self.assertIsInstance(noon.running_start(Fraction(1, 4), Fraction(-1, 2)), Fraction)
        self.assertLess(noon.running_start(0.25), 0)
        self.assertEqual(noon.running_start(1), 1)
        self.assertNotEqual(noon.running_start(0.25, 0), noon.running_start(0.25))

    def test_pause_plateau_and_invalid_parameter_only_evaluate_inside_interval(self):
        self.assertEqual(noon.there_and_back_with_pause(0.5), 1)
        self.assertEqual(noon.there_and_back_with_pause(-1, 1), 0)
        self.assertEqual(noon.there_and_back_with_pause(math.nan), 0)
        with self.assertRaises(ZeroDivisionError):
            noon.there_and_back_with_pause(0.5, 1)

    def test_zero_and_unit_boundary_rules_remain_distinct(self):
        self.assertEqual(noon.exponential_decay(math.nan), 1)
        self.assertEqual(noon.running_start(math.nan), 1)
        self.assertEqual(noon.wiggle(math.nan), 0)
        self.assertEqual(noon.wiggle(math.inf), 0)


if __name__ == '__main__':
    unittest.main()
