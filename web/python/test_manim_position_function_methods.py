import inspect
import sys
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np

import noon


class ManimPositionFunctionMethodTests(unittest.TestCase):
    def test_apply_function_to_position_maps_a_point3d_then_uses_move_to(self):
        value = SimpleNamespace()
        value.get_center = Mock(return_value=noon.Vec2(1.0, -2.0))
        value.move_to = Mock(return_value=value)

        def translate(point):
            self.assertIsInstance(point, np.ndarray)
            self.assertEqual(point.shape, (3,))
            return point + np.asarray((2.0, 3.0, 0.0))

        self.assertIs(noon.Mobject.apply_function_to_position(value, translate), value)
        value.get_center.assert_called_once_with()
        np.testing.assert_array_equal(value.move_to.call_args.args[0], (3.0, 1.0, 0.0))

    def test_apply_function_to_position_falls_back_when_numpy_is_not_loaded(self):
        value = SimpleNamespace()
        value.get_center = Mock(return_value=noon.Vec2(1.0, -2.0))
        value.move_to = Mock(return_value=value)
        with patch.dict(sys.modules, {"numpy": None}):
            self.assertIs(
                noon.Mobject.apply_function_to_position(value, lambda point: point),
                value,
            )
        value.move_to.assert_called_once_with((1.0, -2.0, 0.0))

    def test_apply_function_to_submobject_positions_visits_only_direct_children(self):
        first = SimpleNamespace(apply_function_to_position=Mock())
        second = SimpleNamespace(apply_function_to_position=Mock())
        nested = SimpleNamespace(apply_function_to_position=Mock())
        root = SimpleNamespace(submobjects=[first, second])
        function = Mock()

        self.assertIs(
            noon.Mobject.apply_function_to_submobject_positions(root, function),
            root,
        )
        first.apply_function_to_position.assert_called_once_with(function)
        second.apply_function_to_position.assert_called_once_with(function)
        nested.apply_function_to_position.assert_not_called()

    def test_pinned_mapping_method_signatures(self):
        position = inspect.signature(noon.Mobject.apply_function_to_position)
        self.assertEqual(tuple(position.parameters), ("self", "function"))
        submobjects = inspect.signature(noon.Mobject.apply_function_to_submobject_positions)
        self.assertEqual(tuple(submobjects.parameters), ("self", "function"))


if __name__ == "__main__":
    unittest.main()
