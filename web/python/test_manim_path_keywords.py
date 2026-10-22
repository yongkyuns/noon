"""Pinned public keyword names reach existing Rust-backed adapters."""
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import noon
import _manim_compat as compat
import _manim_path_editing as editing
from _typed_geometry_test_support import identity_only_wrapper


class PathKeywordTests(unittest.TestCase):
    def test_append_points_accepts_new_points_keyword(self):
        value = identity_only_wrapper(compat.VMobject)
        points = [[0., 0., 0.], [1., 2., 0.]]
        with patch.object(editing, "append_points", return_value=value) as operation:
            self.assertIs(value.append_points(new_points=points), value)
        operation.assert_called_once_with(value, points)

    def test_cubic_curve_accepts_handle_keywords(self):
        value = identity_only_wrapper(compat.VMobject)
        handle1, handle2, anchor = (0., 1., 0.), (1., 1., 0.), (1., 0., 0.)
        with patch.object(editing, "add_cubic_bezier_curve_to", return_value=value) as operation:
            self.assertIs(value.add_cubic_bezier_curve_to(
                handle1=handle1, handle2=handle2, anchor=anchor), value)
        operation.assert_called_once_with(value, handle1, handle2, anchor)

    def test_quadratic_curve_accepts_handle_keyword(self):
        value = identity_only_wrapper(compat.VMobject)
        handle, anchor = (0., 1., 0.), (1., 0., 0.)
        with patch.object(editing, "add_quadratic_bezier_curve_to", return_value=value) as operation:
            self.assertIs(value.add_quadratic_bezier_curve_to(handle=handle, anchor=anchor), value)
        operation.assert_called_once_with(value, handle, anchor)

    def test_append_vectorized_mobject_accepts_source_keyword(self):
        value, source = (identity_only_wrapper(compat.VMobject) for _ in range(2))
        with patch.object(editing, "append_vectorized_mobject", return_value=value) as operation:
            self.assertIs(value.append_vectorized_mobject(vectorized_mobject=source), value)
        operation.assert_called_once_with(value, source)

    def test_move_to_accepts_point_or_mobject_keyword_on_inherited_public_classes(self):
        for cls in (noon.Mobject, compat.VMobject, compat.Circle,
                    compat.Square, compat.VectorizedPoint):
            with self.subTest(cls=cls.__name__):
                value = identity_only_wrapper(cls)
                target = noon.Vec2(3., 2.)
                operation = Mock(return_value=value)
                callbacks = SimpleNamespace(_canonical_move_to=operation)
                with patch.object(noon, "_callback_operations", return_value=callbacks):
                    self.assertIs(value.move_to(point_or_mobject=target), value)
                operation.assert_called_once_with(value, target)


if __name__ == "__main__":
    unittest.main()
