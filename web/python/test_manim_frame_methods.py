import inspect
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, call, patch

import noon


class ManimFrameMethodTests(unittest.TestCase):
    def test_align_on_border_uses_rust_critical_point_and_shift(self):
        value = SimpleNamespace()
        value.get_critical_point = Mock(return_value=noon.Vec2(2.0, -2.0))
        value.shift = Mock(return_value=value)
        with patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(noon.Mobject.align_on_border(value, (1.0, -1.0), 0.5), value)
        value.get_critical_point.assert_called_once_with(noon.Vec2(1.0, -1.0))
        value.shift.assert_called_once_with(noon.Vec2(
            noon.DEFAULT_FRAME_WIDTH / 2 - 2.5,
            -noon.DEFAULT_FRAME_HEIGHT / 2 + 2.5,
        ))

    def test_shift_onto_screen_checks_edges_in_pinned_order(self):
        value = SimpleNamespace()
        value.get_edge_center = Mock(side_effect=[
            noon.Vec2(0.0, 5.0),
            noon.Vec2(0.0, 0.0),
            noon.Vec2(0.0, 0.0),
            noon.Vec2(0.0, 0.0),
        ])
        value.to_edge = Mock(return_value=value)
        with patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(noon.Mobject.shift_onto_screen(value, buff=0.25), value)
        self.assertEqual(
            value.get_edge_center.call_args_list,
            [call(direction) for direction in (noon.UP, noon.DOWN, noon.LEFT, noon.RIGHT)],
        )
        value.to_edge.assert_called_once_with(noon.UP, buff=0.25)

    def test_is_off_screen_uses_default_frame_bounds_and_short_circuits(self):
        value = SimpleNamespace()
        value.get_left = Mock(return_value=noon.Vec2(noon.DEFAULT_FRAME_WIDTH / 2 + 0.1, 0.0))
        value.get_right = Mock(side_effect=AssertionError("right edge should not be read"))
        self.assertTrue(noon.Mobject.is_off_screen(value))
        value.get_left.assert_called_once_with()
        value.get_right.assert_not_called()

    def test_pinned_frame_method_signatures(self):
        align = inspect.signature(noon.Mobject.align_on_border)
        self.assertEqual(tuple(align.parameters), ("self", "direction", "buff"))
        self.assertEqual(align.parameters["buff"].default, noon.DEFAULT_MOBJECT_TO_EDGE_BUFFER)
        shift = inspect.signature(noon.Mobject.shift_onto_screen)
        self.assertEqual(tuple(shift.parameters), ("self", "kwargs"))
        offscreen = inspect.signature(noon.Mobject.is_off_screen)
        self.assertEqual(tuple(offscreen.parameters), ("self",))


if __name__ == "__main__":
    unittest.main()
