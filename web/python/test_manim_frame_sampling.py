import unittest

from _manim_frame_sampling import frame_times, logical_endpoint


class ManimFrameSamplingTests(unittest.TestCase):
    def test_integral_duration_excludes_logical_endpoint(self):
        samples = frame_times(1.0, 30.0)

        self.assertEqual(len(samples), 30)
        self.assertEqual(samples[0], 0.0)
        self.assertEqual(samples[-1], 29.0 / 30.0)
        self.assertNotIn(logical_endpoint(1.0), samples)

    def test_fractional_and_subframe_durations_follow_frame_grid(self):
        self.assertEqual(frame_times(0.85, 30.0), tuple(index / 30.0 for index in range(26)))
        self.assertEqual(frame_times(0.01, 30.0), (0.0,))
        self.assertEqual(frame_times(0.0, 30.0), ())

    def test_frame_index_generation_avoids_accumulation_drift(self):
        samples = frame_times(10.0, 60.0)

        self.assertEqual(len(samples), 600)
        self.assertEqual(samples[-1], 599.0 / 60.0)
        self.assertLess(samples[-1], logical_endpoint(10.0))

    def test_invalid_sampling_arguments_are_rejected(self):
        for run_time in (-0.1, float("nan"), float("inf")):
            with self.subTest(run_time=run_time):
                with self.assertRaisesRegex(ValueError, "run_time"):
                    frame_times(run_time, 30.0)

        for frame_rate in (0.0, -30.0, float("nan"), float("inf")):
            with self.subTest(frame_rate=frame_rate):
                with self.assertRaisesRegex(ValueError, "frame_rate"):
                    frame_times(1.0, frame_rate)


if __name__ == "__main__":
    unittest.main()
