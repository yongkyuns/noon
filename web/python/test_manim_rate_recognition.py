"""Structural easing recognition remains inert and rechecks mutable Python code."""
import types
import unittest
from unittest.mock import patch

import _manim_rate_functions as rates


def reverse_smooth():
    return eval('lambda t: smooth(1 - t)', {'smooth': rates.smooth})


class ReverseSmoothRecognitionTests(unittest.TestCase):
    def test_linear_does_not_disassemble_for_each_animation_leaf(self):
        with patch.object(rates.dis, 'get_instructions',
                          side_effect=AssertionError('unnecessary disassembly')):
            for _ in range(1200):
                self.assertFalse(rates.is_reverse_smooth_rate_func(rates.linear))
            self.assertFalse(rates.is_reverse_smooth_rate_func(lambda t: t * t))

    def test_canonical_reverse_is_recognized_without_evaluating_it(self):
        function = reverse_smooth()
        with patch.object(rates.math, 'exp', side_effect=AssertionError('sampled easing')):
            self.assertTrue(rates.is_reverse_smooth_rate_func(function))
        self.assertFalse(rates.is_reverse_smooth_rate_func(
            eval('lambda t: smooth(1 - t) + 0.0', {'smooth': rates.smooth})))

    def test_rebound_globals_and_replaced_code_are_rechecked(self):
        function = reverse_smooth()
        self.assertTrue(rates.is_reverse_smooth_rate_func(function))
        function.__globals__['smooth'] = lambda value: value
        self.assertFalse(rates.is_reverse_smooth_rate_func(function))
        function.__globals__['smooth'] = rates.smooth
        self.assertTrue(rates.is_reverse_smooth_rate_func(function))
        original = function.__code__
        function.__code__ = (lambda t: t).__code__
        self.assertFalse(rates.is_reverse_smooth_rate_func(function))
        function.__code__ = original
        self.assertTrue(rates.is_reverse_smooth_rate_func(function))

    def test_unused_code_names_do_not_narrow_existing_admitted_form(self):
        function = reverse_smooth()
        code = function.__code__.replace(co_names=function.__code__.co_names + ('unused',))
        function = types.FunctionType(code, function.__globals__)
        self.assertTrue(rates.is_reverse_smooth_rate_func(function))

    def test_defaults_closures_and_callable_objects_remain_unrecognized(self):
        self.assertFalse(rates.is_reverse_smooth_rate_func(
            eval('lambda t=0: smooth(1 - t)', {'smooth': rates.smooth})))
        smooth = rates.smooth
        self.assertFalse(rates.is_reverse_smooth_rate_func(lambda t: smooth(1 - t)))
        class Callable:
            def __call__(self, value):
                raise AssertionError('called arbitrary easing')
        self.assertFalse(rates.is_reverse_smooth_rate_func(Callable()))


if __name__ == '__main__':
    unittest.main()
