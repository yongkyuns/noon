"""Ownership and argument forwarding tests for static implicit contours."""
import math
import unittest
from unittest.mock import Mock, patch

import _manim_implicit as implicit


class Proxy:
    def __init__(self, callback):
        self.callback = callback
        self.destroyed = 0

    def destroy(self):
        self.destroyed += 1


class ImplicitAdapterTests(unittest.TestCase):
    def setUp(self):
        self.array_bridge = patch.object(implicit._plot, "_to_js", lambda values: values)
        self.array_bridge.start()
        self.addCleanup(self.array_bridge.stop)

    def test_prepare_destroys_proxy_after_success_and_forwards_arguments(self):
        proxy = None
        received = []

        def create(callback):
            nonlocal proxy
            proxy = Proxy(callback)
            return proxy

        def factory(callback_proxy, *args):
            received.append((callback_proxy, args))
            return "options"

        with patch.object(implicit, "_create_proxy", side_effect=create):
            result = implicit._prepare(factory, lambda x, y: x + y, ("x", "y"), 3, 40, True)

        self.assertEqual(result, "options")
        self.assertEqual(received, [(proxy, ("x", "y", 3, 40, True))])
        self.assertEqual(proxy.destroyed, 1)

    def test_prepare_destroys_proxy_after_factory_failure(self):
        proxy = None

        def create(callback):
            nonlocal proxy
            proxy = Proxy(callback)
            return proxy

        failure = RuntimeError("factory failed")
        with patch.object(implicit, "_create_proxy", side_effect=create):
            with self.assertRaises(RuntimeError) as caught:
                implicit._prepare(Mock(side_effect=failure), lambda x, y: 0.0, (), 5, 10, False)

        self.assertIs(caught.exception, failure)
        self.assertEqual(proxy.destroyed, 1)

    def test_callback_exception_identity_is_preserved_and_proxy_is_destroyed(self):
        proxy = None
        failure = ValueError("user callback failed")

        def create(callback):
            nonlocal proxy
            proxy = Proxy(callback)
            return proxy

        def factory(callback_proxy, *args):
            callback_proxy.callback(1.0, 2.0)

        def function(_, __):
            raise failure

        with patch.object(implicit, "_create_proxy", side_effect=create):
            with self.assertRaises(ValueError) as caught:
                implicit._prepare(factory, function, (), 5, 10, False)

        self.assertIs(caught.exception, failure)
        self.assertEqual(proxy.destroyed, 1)

    def test_nan_result_is_forwarded_to_rust_callback(self):
        proxy = None
        observed = []

        def create(callback):
            nonlocal proxy
            proxy = Proxy(callback)
            return proxy

        def factory(callback_proxy, *args):
            observed.append(callback_proxy.callback(float("nan"), 2.0))
            return "options"

        with patch.object(implicit, "_create_proxy", side_effect=create):
            implicit._prepare(factory, lambda x, y: float("nan"), (), 5, 10, True)

        self.assertTrue(math.isnan(observed[0]))
        self.assertEqual(proxy.destroyed, 1)

    def test_bool_depth_and_budget_are_rejected_before_proxy_creation(self):
        for name, depth, quads in (("min_depth", True, 10), ("max_quads", 5, True)):
            with self.subTest(name=name), patch.object(implicit, "_create_proxy") as create:
                with self.assertRaisesRegex(TypeError, name + r" must be an integer"):
                    implicit._prepare(Mock(), lambda x, y: 0.0, (), depth, quads, False)
                create.assert_not_called()

    def test_constructor_does_not_attach_after_preparation_error(self):
        failure = ValueError("invalid contour")
        host = Mock()
        with patch.object(implicit._plot._shared, "_geometry_options", host), \
             patch.object(implicit, "_prepare", side_effect=failure), \
             patch.object(implicit._plot, "_curve") as curve:
            with self.assertRaises(ValueError) as caught:
                implicit.ImplicitFunction(lambda x, y: x + y)

        self.assertIs(caught.exception, failure)
        curve.assert_not_called()

    def test_axes_captures_and_frees_one_frame_and_delegates_curve_options(self):
        frame = Mock()
        frame.implicitPlot.return_value = "options"
        axes = Mock()
        axes._coordinate_frame.return_value = frame
        result_wrapper = object()

        with patch.object(implicit, "_prepare", return_value="options") as prepare, \
             patch.object(implicit._plot, "_curve", return_value=result_wrapper) as curve:
            result = implicit.plot_implicit_curve(
                axes, lambda x, y: x - y, 2, 80, use_smoothing=False,
                color="blue", stroke_width=3,
            )

        self.assertIs(result, result_wrapper)
        axes._coordinate_frame.assert_called_once_with()
        frame.free.assert_called_once_with()
        prepare.assert_called_once()
        self.assertIs(prepare.call_args.args[0], frame.implicitPlot)
        self.assertEqual(prepare.call_args.args[2:], ((), 2, 80, False))
        curve.assert_called_once()
        self.assertIs(curve.call_args.args[1], "options")
        self.assertEqual(curve.call_args.args[2:], ("blue", {"stroke_width": 3}))

    def test_constructor_does_not_store_python_callable_on_wrapper(self):
        host = Mock()
        with patch.object(implicit._plot._shared, "_geometry_options", host), \
             patch.object(implicit, "_prepare", return_value="options"), \
             patch.object(implicit._plot, "_curve", side_effect=lambda wrapper, options, color, kwargs: wrapper):
            function = lambda x, y: x + y
            wrapper = implicit.ImplicitFunction(function)

        self.assertFalse(any(value is function for value in vars(wrapper).values()))


if __name__ == "__main__":
    unittest.main()
