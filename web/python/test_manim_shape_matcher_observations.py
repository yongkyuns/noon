"""Shape matchers read a single bounds value from their owning Scene."""
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import _manim_shared_geometry as geometry


class ShapeMatcherObservationTests(unittest.TestCase):
    def leaf(self):
        target = object.__new__(geometry._compat.Rectangle)
        handle = Mock()
        geometry._shared._attach_shared_handle(target, handle)
        return target, handle

    def test_detached_target_uses_authored_bounds(self):
        target, handle = self.leaf()
        self.assertEqual(geometry._shape_matcher_bounds(target), (handle.shapeMatcherBounds.return_value, None))
        handle.shapeMatcherBounds.assert_called_once_with()

    def test_rejected_scene_query_does_not_use_authored_bounds(self):
        target, handle = self.leaf()
        context = SimpleNamespace(mobjectShapeMatcherBounds=Mock(side_effect=RuntimeError("execution is transferred")))
        with patch.object(geometry._shared, "_path_authoring_context", return_value=context):
            with self.assertRaisesRegex(RuntimeError, "execution is transferred"):
                geometry._shape_matcher_bounds(target)
        context.mobjectShapeMatcherBounds.assert_called_once_with(handle)
        handle.shapeMatcherBounds.assert_not_called()

    def test_family_uses_owning_scene(self):
        target = object.__new__(geometry._compat.VGroup)
        target._semantic_family_handle = Mock()
        context = SimpleNamespace(familyShapeMatcherBounds=Mock())
        with patch.object(geometry._shared, "_group_layout_context", return_value=context):
            self.assertEqual(geometry._shape_matcher_bounds(target), (context.familyShapeMatcherBounds.return_value, context))
        context.familyShapeMatcherBounds.assert_called_once_with(target._semantic_family_handle)
        target._semantic_family_handle.shapeMatcherBounds.assert_not_called()

    def test_cross_prepares_both_children_from_one_scene_observation(self):
        target, handle = self.leaf()
        bounds = (-1, -1, 1, 1)
        context = SimpleNamespace(mobjectShapeMatcherBounds=Mock(return_value=bounds))
        candidates = (Mock(), Mock())
        factory = SimpleNamespace(crossLines=Mock(return_value=candidates))
        with (
            patch.object(geometry._shared, "_path_authoring_context", return_value=context),
            patch.object(geometry._shared, "_create_geometry_handle", object()),
            patch.object(geometry._shared, "_geometry_options", factory),
            patch.object(geometry._shared, "_attach_geometry_options") as attach,
            patch.object(geometry._compat.VGroup, "__init__", return_value=None),
        ):
            geometry.Cross(target, scale_factor=1.5)
        context.mobjectShapeMatcherBounds.assert_called_once_with(handle)
        factory.crossLines.assert_called_once_with(bounds, 1.5)
        self.assertEqual([call.args[1] for call in attach.call_args_list], list(candidates))
        self.assertTrue(all(call.kwargs["context"] is context for call in attach.call_args_list))


if __name__ == "__main__":
    unittest.main()
