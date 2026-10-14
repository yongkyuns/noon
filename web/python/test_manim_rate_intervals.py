import math
import unittest
try:
    import numpy as np
except ImportError:
    np = None

import noon
import _manim_rate_functions as rates


class RateIntervals(unittest.TestCase):
    def test_public_rate_module_preserves_callable_identity_and_shared_recognition(self):
        from types import ModuleType
        self.assertIsInstance(noon.rate_functions, ModuleType)
        self.assertIs(noon.rate_functions, rates)
        for power in ("quad", "cubic", "quart", "quint"):
            for direction in ("in", "out", "in_out"):
                name = f"ease_{direction}_{power}"
                function = getattr(noon.rate_functions, name)
                self.assertIs(function, getattr(rates, name))
                self.assertNotIn(name, noon.__all__)
                self.assertFalse(hasattr(noon, name))
                self.assertEqual(rates.easing_from_rate_func(function), name)

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
        for value in [0.0, 0.25, 1.0]:
            self.assertIs(type(noon.smooth(value)), float)
        self.assertAlmostEqual(noon.there_and_back(0.5), 1)

    def test_wrapping_preserves_shared_rate_identity_and_reverse_expression(self):
        for name in ["linear", "smooth", "rush_into", "rush_from", "there_and_back"]:
            function = getattr(noon, name)
            self.assertEqual(function.__name__, name)
            self.assertEqual(rates.easing_from_rate_func(function), name)
        expression = eval("lambda t: smooth(1 - t)", {"smooth": noon.smooth})
        self.assertTrue(rates.is_reverse_smooth_rate_func(expression))

    def test_polynomial_rates_preserve_numeric_types_and_shared_ids(self):
        from fractions import Fraction
        for name, quarter in [("smoothstep", Fraction(5, 32)),
                              ("smootherstep", Fraction(53, 512)),
                              ("smoothererstep", Fraction(289, 4096))]:
            function = getattr(noon, name)
            result = function(Fraction(1, 4))
            self.assertIsInstance(result, Fraction)
            self.assertEqual(result, quarter)
            self.assertEqual(function(0.5), 0.5)
            self.assertEqual(function(-math.inf), 0)
            self.assertEqual(function(math.inf), 1)
            self.assertEqual(function(math.nan), 1)
            self.assertEqual(rates.easing_from_rate_func(function), name)
            self.assertIn(name, noon.__all__)

    def test_slow_and_double_smooth_use_shared_ids_and_original_boundaries(self):
        for name in ["slow_into", "double_smooth"]:
            function = getattr(noon, name)
            self.assertIn(name, noon.__all__)
            self.assertEqual(rates.easing_from_rate_func(function), name)
            self.assertEqual(function(-math.inf), 0)
            self.assertEqual(function(math.nan), 1)
            self.assertEqual(function(math.inf), 1)
            with self.assertRaises(TypeError): function("0.5")
    @unittest.skipIf(np is None, "Numeric NumPy rate contracts run in the pinned utility lane")
    def test_numpy_rate_values_types_and_decimal_errors(self):
        self.assertAlmostEqual(noon.slow_into(0.25), 0.6614378277661477)
        self.assertEqual(noon.double_smooth(0.25), 0.25)
        self.assertEqual(noon.double_smooth(0.5), 0.5)
        self.assertEqual(noon.double_smooth(0.75), 0.75)
        from decimal import Decimal
        self.assertIs(type(noon.double_smooth(0.25)), np.float64)
        with self.assertRaises(TypeError): noon.double_smooth(Decimal("0.25"))

    def test_callable_wrappers_forward_arguments_and_do_not_eagerly_call_users(self):
        calls = []
        operand = object()
        def callback(t, arg, *, token):
            calls.append((t, arg, token))
            return t + 2
        reduced = noon.not_quite_there(callback, proportion=0.25)
        compressed = noon.squish_rate_func(callback, a=0.2, b=0.8)
        self.assertEqual(calls, [])
        self.assertEqual(reduced(0.5, operand, token=operand), 0.625)
        self.assertEqual(compressed(0.5, operand, token=operand), 2.5)
        self.assertEqual([call[0] for call in calls], [0.5, 0.4999999999999999])
        self.assertTrue(all(arg is operand and token is operand for _, arg, token in calls))
        self.assertIn("not_quite_there", noon.__all__)
        self.assertIn("squish_rate_func", noon.__all__)
        for function in [reduced, compressed]:
            with self.assertRaises(NotImplementedError): rates.easing_from_rate_func(function)

    def test_equal_endpoints_return_original_a_without_calling_callback(self):
        from fractions import Fraction
        endpoint = Fraction(1, 3)
        def callback(*args, **kwargs):
            raise AssertionError("equal endpoints must not invoke the callback")
        compressed = noon.squish_rate_func(callback, endpoint, Fraction(1, 3))
        for value in [-math.inf, 0.0, math.nan, object()]:
            self.assertIs(compressed(value, token=object()), endpoint)

    def test_wrapper_callback_errors_and_reversed_interval_behavior(self):
        failure = ValueError("user callback failed")
        def callback(t):
            raise failure
        for function in [noon.not_quite_there(callback), noon.squish_rate_func(callback)]:
            with self.assertRaises(ValueError) as raised: function(0.5)
            self.assertIs(raised.exception, failure)
        reversed_interval = noon.squish_rate_func(noon.linear, a=0.8, b=0.2)
        self.assertEqual([reversed_interval(t) for t in [0.0, 0.5, 1.0]], [0.0, 0.0, 1.0])
        self.assertEqual(noon.not_quite_there()(0.5), 0.35)


if __name__ == "__main__": unittest.main()
