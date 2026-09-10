"""The wrapper owns argument conversion only; all geometry stays in Rust."""
import unittest
from unittest.mock import Mock, patch

import _manim_shared_geometry as geometry


class BooleanGeometryTests(unittest.TestCase):
    def setUp(self):
        self.operands = [object.__new__(geometry._compat.VMobject) for _ in range(2)]
        self.selection = Mock()
        self.options = Mock()
        self.host = Mock()
        self.host.booleanOperands.return_value = self.selection
        self.host.booleanGeometry.return_value = self.options
        self.patches = [
            patch.object(geometry._shared, "_geometry_options", self.host),
            patch.object(geometry._shared, "_handle_for", side_effect=lambda x: x),
            patch.object(geometry._shared, "_live_constructor_context", return_value=None),
            patch.object(geometry._shared, "_apply_shared_constructor_options"),
            patch.object(geometry._shared, "_attach_geometry_options"),
        ]
        self.mocks = [p.start() for p in self.patches]
        for p in self.patches:
            self.addCleanup(p.stop)

    def test_all_constructors_dispatch_handles_without_observing_geometry(self):
        for cls in (geometry.Union, geometry.Intersection, geometry.Difference, geometry.Exclusion):
            obj = cls(*self.operands, fill_opacity=0.5)
            self.host.booleanGeometry.assert_called_with(cls.__name__, self.selection)
            self.mocks[3].assert_called_with(self.options, {"fill_opacity": 0.5})
            self.mocks[4].assert_called_with(obj, self.options, cls.__name__)
        self.assertEqual(self.selection.free.call_count, 4)

    def test_live_context_captures_all_operands_in_one_call(self):
        context = Mock()
        context.beginBooleanGeometry.return_value = self.options
        self.mocks[2].return_value = context
        geometry.Union(*self.operands)
        context.beginBooleanGeometry.assert_called_once_with("Union", self.selection)
        self.host.booleanGeometry.assert_not_called()

    def test_failed_shared_validation_publishes_nothing_and_frees_inputs(self):
        self.host.booleanGeometry.side_effect = ValueError("bad operands")
        with self.assertRaises(ValueError):
            geometry.Union(*self.operands)
        self.selection.free.assert_called_once()
        self.mocks[4].assert_not_called()

    def test_invalid_constructor_options_release_inert_result(self):
        self.mocks[3].side_effect = ValueError("bad style")
        with self.assertRaises(ValueError):
            geometry.Union(*self.operands, stroke_width=-1)
        self.options.free.assert_called_once()
        self.mocks[4].assert_not_called()

    def test_invalid_operand_never_calls_geometry_host(self):
        with self.assertRaises(TypeError):
            geometry.Union(self.operands[0], object())
        self.host.booleanOperands.assert_not_called()


if __name__ == "__main__":
    unittest.main()
