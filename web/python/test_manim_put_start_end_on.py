import unittest
import warnings
from unittest.mock import ANY, Mock

import noon
from _manim_shared_geometry import _put_start_and_end_on


class ManimPutStartAndEndOnTests(unittest.TestCase):
    def test_scales_rotates_and_shifts_about_the_original_start(self):
        value = Mock()
        value.get_start_and_end.return_value = (noon.Vec2(-1, 0), noon.Vec2(1, 0))

        self.assertIs(_put_start_and_end_on(value, (2, 3, 0), (2, 7, 0)), value)

        value.scale.assert_called_once_with(2.0, about_point=noon.Vec2(-1, 0))
        value.rotate.assert_called_once_with(ANY, about_point=noon.Vec2(-1, 0))
        self.assertAlmostEqual(value.rotate.call_args.args[0], 1.5707963267948966)
        value.shift.assert_called_once_with(noon.Vec2(3, 3))

    def test_closed_path_warns_and_moves_its_start_to_the_requested_start(self):
        class ClosedPathDouble(Mock):
            pass

        value = ClosedPathDouble()
        value.get_start_and_end.return_value = (noon.Vec2(0.5, -0.25), noon.Vec2(0.5, -0.25))

        with warnings.catch_warnings(record=True) as emitted:
            warnings.simplefilter("always")
            result = _put_start_and_end_on(value, (2, 1), (4, 1))

        self.assertIs(result, value)
        self.assertEqual(len(emitted), 1)
        self.assertEqual(emitted[0].category, UserWarning)
        self.assertEqual(
            str(emitted[0].message),
            "put_start_and_end_on has been called on a closed loop or zero-length mobject. "
            "ClosedPathDouble will be shifted to start point instead.",
        )
        value.shift.assert_called_once_with(noon.Vec2(1.5, 1.25))
        value.scale.assert_not_called()
        value.rotate.assert_not_called()

    def test_nonplanar_target_is_rejected_before_path_queries_or_mutation(self):
        value = Mock()
        with self.assertRaisesRegex(NotImplementedError, "z must be 0"):
            _put_start_and_end_on(value, (0, 0, 0), (0, 1, 2))
        value.get_start_and_end.assert_not_called()
        value.scale.assert_not_called()
        value.rotate.assert_not_called()
        value.shift.assert_not_called()

    def test_public_method_is_declared_on_mobject(self):
        self.assertIn("put_start_and_end_on", noon.Mobject.__dict__)


if __name__ == "__main__":
    unittest.main()
