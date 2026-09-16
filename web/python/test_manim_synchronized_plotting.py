"""Adapter projections and cleanup only; Rust/Pyodide tests own numerical rules."""
import unittest
from unittest.mock import patch
from types import SimpleNamespace
import _manim_plotting as plotting


class Owned:
    def __init__(self, **methods):
        self.__dict__.update(methods)
        self.freed = 0

    def free(self):
        self.freed += 1


class SynchronizedAdapterTests(unittest.TestCase):
    def setUp(self):
        self.plan = Owned(
            seriesCount=lambda: 2,
            seriesPoints=lambda i: ((-5, -1, 5, 1), (-5, 1, 5, -1))[i],
            dataTimes=lambda: (0, 10), cursorPoints=lambda: (-5, 0, 5, 0),
            keyTimes=lambda: (0, 6), durations=lambda: (6,), runTime=lambda: 6,
        )
        self.calls = []
        self.frame = Owned(synchronizedSeriesPlan=self.prepare)
        self.axes = SimpleNamespace(_coordinate_frame=lambda: self.frame)
        self.addCleanup(patch.stopall)
        patch.object(plotting, '_to_js', side_effect=lambda x: x).start()
        patch.object(plotting, 'engine_call', side_effect=lambda fn, *args: fn(*args)).start()

    def prepare(self, *args):
        self.calls.append(args)
        return self.plan

    def invoke(self, series=None, **kwargs):
        if series is None:
            series = (((0, 1), (10, 2)), ((-1, 3), (12, 4)))
        return plotting.Axes.synchronized_series_plan(
            self.axes, series, time_range=kwargs.get('time_range', (0, 10)),
            run_time=kwargs.get('run_time', 6),
        )

    def test_one_frame_and_typed_payload_preserve_order_and_counts(self):
        result = self.invoke()
        self.assertEqual(self.calls, [([0., 1., 10., 2., -1., 3., 12., 4.], [2., 2.], [0., 10.], 6.)])
        self.assertEqual(result.series_points[0][0], plotting._base.Vec2(-5, -1))
        self.assertEqual(result.series_points[1][-1], plotting._base.Vec2(5, -1))
        self.assertEqual(result.data_times, (0., 10.))
        self.assertEqual(result.durations, (6.,))
        self.assertEqual((self.plan.freed, self.frame.freed), (1, 1))

    def test_nested_iterators_are_consumed_once(self):
        rows = (iter(row) for row in (((0, 1), (10, 2)), ((-1, 3), (12, 4))))
        result = self.invoke(rows, time_range=iter((0, 10)), run_time='6')
        self.assertEqual(len(result.series_points), 2)
        self.assertEqual(self.calls[0][1], [2., 2.])

    def test_bad_host_value_releases_frame_before_rust_call(self):
        with self.assertRaises((TypeError, ValueError)):
            self.invoke((((0, 'not numeric'), (10, 2)),))
        self.assertEqual(self.calls, [])
        self.assertEqual((self.plan.freed, self.frame.freed), (0, 1))

    def test_rust_rejection_is_propagated_without_fabricating_a_plan(self):
        error = ValueError('outside coverage')
        def reject(*args):
            raise error
        self.frame.synchronizedSeriesPlan = reject
        with self.assertRaises(ValueError) as caught:
            self.invoke()
        self.assertIs(caught.exception, error)
        self.assertEqual((self.plan.freed, self.frame.freed), (0, 1))

    def test_projection_error_releases_both_handles(self):
        error = RuntimeError('projection failed')
        def reject(index):
            raise error
        self.plan.seriesPoints = reject
        with self.assertRaises(RuntimeError) as caught:
            self.invoke()
        self.assertIs(caught.exception, error)
        self.assertEqual((self.plan.freed, self.frame.freed), (1, 1))

    def test_result_is_an_immutable_value_not_a_live_plan_handle(self):
        result = self.invoke()
        with self.assertRaises(AttributeError):
            result.run_time = 7
        self.assertIsInstance(result.series_points, tuple)
        self.assertTrue(all(isinstance(row, tuple) for row in result.series_points))
        self.assertFalse(hasattr(result, 'free'))


if __name__ == '__main__':
    unittest.main()
