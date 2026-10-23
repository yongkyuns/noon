import unittest
from unittest.mock import Mock, patch

import noon
from _manim_path_editing import apply_over_attr_arrays


class ManimApplyOverAttrArraysTests(unittest.TestCase):
    def _value(self, attrs=("points",)):
        value = Mock()
        value.get_array_attrs.return_value = list(attrs)
        return value

    def test_maps_full_xyz_snapshot_and_preserves_all_coordinates(self):
        value = self._value()
        query = Mock()
        query.points3D.return_value = [1.0, 2.0, .25, 3.0, 4.0, .75]
        edit = Mock(return_value=value)

        def translate(points):
            rows = [[float(component) for component in point] for point in points]
            self.assertEqual(rows, [[1.0, 2.0, .25], [3.0, 4.0, .75]])
            return [[x + 5.0, y - 2.0, z + 1.] for x, y, z in rows]

        with patch("_manim_path_queries._query", return_value=query), \
             patch("_manim_path_editing.edit_points", edit), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(apply_over_attr_arrays(value, translate), value)

        mapped = edit.call_args.args[3]
        self.assertEqual(
            [[float(component) for component in point] for point in mapped],
            [[6.0, 0.0, 1.25], [8.0, 2.0, 1.75]],
        )
        edit.assert_called_once_with(
            value, "setPoints", "liveSetPoints", mapped, array=True
        )
        query.free.assert_called_once_with()

    def test_does_not_query_or_call_function_when_no_array_attributes_exist(self):
        value = self._value(attrs=())
        func = Mock()
        with patch("_manim_path_queries._query") as query:
            self.assertIs(apply_over_attr_arrays(value, func), value)
        query.assert_not_called()
        func.assert_not_called()

    def test_rejects_unmodeled_attributes_before_query_or_callback(self):
        value = self._value(attrs=("points", "rgbas"))
        func = Mock()
        with patch("_manim_path_queries._query") as query:
            with self.assertRaisesRegex(NotImplementedError, "only the shared Rust points"):
                apply_over_attr_arrays(value, func)
        query.assert_not_called()
        func.assert_not_called()

    def test_rejects_staged_callbacks_before_query_or_callback(self):
        value = self._value()
        func = Mock()
        with patch("_manim_updaters.canonical_callback_phase_active", return_value=True), \
             patch("_manim_path_queries._query") as query:
            with self.assertRaisesRegex(NotImplementedError, "staged callbacks"):
                apply_over_attr_arrays(value, func)
        query.assert_not_called()
        func.assert_not_called()

    def test_callback_failure_releases_the_snapshot_without_publishing(self):
        value = self._value()
        query = Mock()
        query.points3D.return_value = [1.0, 2.0, .25]
        edit = Mock()

        def fail(_points):
            raise RuntimeError("mapping failed")

        with patch("_manim_path_queries._query", return_value=query), \
             patch("_manim_path_editing.edit_points", edit), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            with self.assertRaisesRegex(RuntimeError, "mapping failed"):
                apply_over_attr_arrays(value, fail)
        query.free.assert_called_once_with()
        edit.assert_not_called()

    def test_public_method_is_declared_on_mobject(self):
        self.assertIn("apply_over_attr_arrays", noon.Mobject.__dict__)


if __name__ == "__main__":
    unittest.main()
