"""Adapter ownership/shape checks; numerical gap behavior is tested in Rust/Pyodide."""
import unittest
from unittest.mock import Mock, patch
import _manim_plotting as plotting


class GappedPlottingAdapterTests(unittest.TestCase):
    def setUp(self):
        self.axes = object.__new__(plotting.Axes)
        self.points = [[(0, 1), None, (2, 3)], [(0, 4), (1, 5), (2, 6)]]
        self.segments = [[None, None], [(0, 4, 1, 5), (1, 5, 2, 6)]]
        self.plan = Mock()
        self.plan.seriesCount.return_value = 2
        self.plan.seriesPoints.side_effect = lambda i: self.points[i]
        self.plan.seriesSegments.side_effect = lambda i: self.segments[i]
        self.plan.dataTimes.return_value = (0, 5, 10)
        self.plan.cursorPoints.return_value = (0, 0, 1, 0, 2, 0)
        self.plan.keyTimes.return_value = (0, 3, 6)
        self.plan.durations.return_value = (3, 3)
        self.plan.runTime.return_value = 6
        self.frame = Mock()
        self.frame.gappedSeriesPlan.return_value = self.plan
        self.frame_patch = patch.object(plotting.Axes, "_coordinate_frame", return_value=self.frame)
        self.array_patch = patch.object(plotting, "_array", side_effect=lambda values: list(map(float, values)))
        self.frame_patch.start()
        self.array_patch.start()
        self.addCleanup(self.frame_patch.stop)
        self.addCleanup(self.array_patch.stop)

    def prepare(self, series=None, breaks=None):
        return self.axes.gapped_series_plan(
            series if series is not None else (((0, 1), (10, 3)), ((0, 4), (10, 6))),
            break_after=breaks if breaks is not None else ((0,), ()),
            time_range=(0, 10), run_time=6,
        )

    def test_preserves_missing_points_and_segments_without_sentinel_geometry(self):
        result = self.prepare()
        self.assertIsNone(result.series_points[0][1])
        self.assertEqual(result.series_segments[0], (None, None))
        self.assertEqual(tuple(result.series_segments[1][0][0]), (0.0, 4.0))
        self.assertEqual(tuple(result.series_segments[1][0][1]), (1.0, 5.0))
        self.frame.free.assert_called_once()
        self.plan.free.assert_called_once()

    def test_normalizes_nested_iterables_once_and_preserves_payload_order(self):
        result = self.prepare(
            (iter(row) for row in (((0, 1), (10, 3)), ((0, 4), (10, 6)))),
            (iter(row) for row in ((0,), ())),
        )
        self.frame.gappedSeriesPlan.assert_called_once_with(
            [0.0, 1.0, 10.0, 3.0, 0.0, 4.0, 10.0, 6.0], [2.0, 2.0],
            [0.0], [1.0, 0.0], [0.0, 10.0], 6.0,
        )
        self.assertEqual(result.data_times, (0.0, 5.0, 10.0))

    def test_failure_preserves_exception_identity_and_releases_frame(self):
        error = ValueError("invalid break index")
        self.frame.gappedSeriesPlan.side_effect = error
        with self.assertRaises(ValueError) as observed:
            self.prepare()
        self.assertIs(observed.exception, error)
        self.frame.free.assert_called_once()
        self.plan.free.assert_not_called()

    def test_projection_failure_releases_both_disposable_handles(self):
        error = RuntimeError("projection failed")
        self.plan.seriesSegments.side_effect = error
        with self.assertRaises(RuntimeError) as observed:
            self.prepare()
        self.assertIs(observed.exception, error)
        self.frame.free.assert_called_once()
        self.plan.free.assert_called_once()

    def test_indices_use_integer_protocol_without_boolean_or_float_coercion(self):
        for value in (True, 1.5, "1"):
            with self.assertRaises(TypeError):
                self.prepare(breaks=((value,), ()))
        self.frame.gappedSeriesPlan.assert_not_called()
        self.assertEqual(self.frame.free.call_count, 3)

    def test_immutable_result_does_not_retain_mutable_host_arrays(self):
        result = self.prepare()
        self.points[0][0] = (9, 9)
        self.segments[1][0] = (9, 9, 9, 9)
        self.assertEqual(tuple(result.series_points[0][0]), (0.0, 1.0))
        self.assertEqual(tuple(result.series_segments[1][0][0]), (0.0, 4.0))
        with self.assertRaises(TypeError):
            result.series_points[0][0] = None


if __name__ == "__main__":
    unittest.main()
