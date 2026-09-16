"""Adapter/lifetime checks. Rust and real Pyodide tests own numerical semantics."""
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch
import _manim_plotting as plotting


class PresentationAdapterTests(unittest.TestCase):
    def setUp(self):
        self.array = patch.object(plotting, "_to_js", lambda values: values)
        self.array.start()
        self.addCleanup(self.array.stop)

    def line(self, plan):
        frame = SimpleNamespace(numberLabelPlan=Mock(return_value=plan), free=Mock())
        return SimpleNamespace(_coordinate_frame=lambda: frame), frame

    def labels(self):
        return SimpleNamespace(numbers=lambda: [4, 2], texts=lambda: ["4.00", "2.00"],
                               points=lambda: [10, 20, 30, 40], free=Mock())

    def test_labels_project_shared_values_in_order_and_release_both_handles(self):
        plan = self.labels()
        line, frame = self.line(plan)
        result = plotting.NumberLine.label_plan(line, [4, 2], decimal_places=2)
        frame.numberLabelPlan.assert_called_once_with([4.0, 2.0], False, 2, True)
        self.assertEqual(tuple(label.text for label in result), ("4.00", "2.00"))
        self.assertEqual(tuple(result[0].point), (10.0, 20.0))
        frame.free.assert_called_once()
        plan.free.assert_called_once()

    def test_automatic_and_explicit_empty_are_distinct(self):
        for values, automatic in ((None, True), ([], False)):
            line, frame = self.line(SimpleNamespace(numbers=lambda: [], texts=lambda: [],
                                                    points=lambda: [], free=Mock()))
            self.assertEqual(plotting.NumberLine.label_plan(line, values), ())
            frame.numberLabelPlan.assert_called_once_with([], automatic, 0, True)

    def test_label_projection_failure_releases_plan_and_frame(self):
        plan = self.labels()
        error = ValueError("text projection failed")
        plan.texts = Mock(side_effect=error)
        line, frame = self.line(plan)
        with self.assertRaises(ValueError) as caught:
            plotting.NumberLine.label_plan(line)
        self.assertIs(caught.exception, error)
        plan.free.assert_called_once()
        frame.free.assert_called_once()

    def test_precision_coercion_fails_before_observing_scene(self):
        line = SimpleNamespace(_coordinate_frame=Mock())
        for precision in (True, 1.5, "2"):
            with self.assertRaises(TypeError):
                plotting.NumberLine.label_plan(line, decimal_places=precision)
        line._coordinate_frame.assert_not_called()

    def test_timing_is_forwarded_without_python_normalization(self):
        plan = SimpleNamespace(points=lambda: [1, 2, 3, 4], cursorPoints=lambda: [1, 0, 3, 0],
                               keyTimes=lambda: [0, 7], durations=lambda: [7], runTime=lambda: 7,
                               free=Mock())
        frame = SimpleNamespace(timeSeriesPlan=Mock(return_value=plan), free=Mock())
        axes = SimpleNamespace(_coordinate_frame=lambda: frame)
        result = plotting.Axes.time_series_plan(axes, [(100, 4), (101, 5)], run_time=7)
        frame.timeSeriesPlan.assert_called_once_with([100.0, 4.0, 101.0, 5.0], 7.0)
        self.assertEqual(result.key_times, (0.0, 7.0))
        self.assertEqual(result.durations, (7.0,))
        self.assertEqual(tuple(result.points[-1]), (3.0, 4.0))
        with self.assertRaises(AttributeError):
            result.run_time = 3
        frame.free.assert_called_once()
        plan.free.assert_called_once()

    def test_bad_input_conversion_releases_observation(self):
        frame = SimpleNamespace(timeSeriesPlan=Mock(), free=Mock())
        axes = SimpleNamespace(_coordinate_frame=lambda: frame)
        with self.assertRaises((ValueError, TypeError)):
            plotting.Axes.time_series_plan(axes, [(1, 2), (3, "bad")], run_time=6)
        frame.timeSeriesPlan.assert_not_called()
        frame.free.assert_called_once()


if __name__ == "__main__":
    unittest.main()
