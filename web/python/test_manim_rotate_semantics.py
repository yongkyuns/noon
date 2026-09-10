"""Wrapper forwarding only; Rust and pinned Manim probes qualify pivot geometry."""
import math
import unittest
from unittest.mock import Mock, patch

import noon
import _manim_compat as compat
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class ManimRotateSemanticsTests(unittest.TestCase):
    def test_object_and_family_pivots_use_the_same_authored_or_live_operation(self):
        for cls in (compat.VMobject, compat.VGroup):
            for live in (False, True):
                with self.subTest(cls=cls, live=live):
                    value = identity_only_wrapper(cls, submobjects=[])
                    anchor, context = Mock(), (Mock() if live else None)
                    value.get_center = Mock(side_effect=AssertionError("Rust resolves pivots"))
                    with patch.object(handles, "_layout_anchor", return_value=anchor), \
                         patch.object(handles, "_live_mutation_context", return_value=context), \
                         patch.object(handles, "_group_live_layout_context", return_value=context):
                        self.assertIs(value.rotate(math.pi, about_edge=noon.LEFT), value)
                        self.assertIs(value.rotate_about_origin(0.5), value)
                        self.assertIs(value.flip(noon.UP, about_point=(2, 3)), value)
                    owner = context if live else anchor
                    prefix = (anchor,) if live else ()
                    rotation = getattr(owner, "liveRotateLayout" if live else "rotate")
                    self.assertEqual(rotation.call_args_list, [
                        unittest.mock.call(*prefix, math.pi, -1., 0., False),
                        unittest.mock.call(*prefix, .5, 0., 0., True)])
                    getattr(owner, "liveFlipLayout" if live else "flip").assert_called_once_with(
                        *prefix, 0., 1., 0., 2., 3., True)

    def test_scale_pivots_forward_for_objects_and_families(self):
        for cls in (compat.VMobject, compat.VGroup):
            for live in (False, True):
                value = identity_only_wrapper(cls, submobjects=[])
                anchor, context = Mock(), (Mock() if live else None)
                with patch.object(handles, "_layout_anchor", return_value=anchor), \
                     patch.object(handles, "_handle_for", return_value=Mock()), \
                     patch.object(handles, "_live_mutation_context", return_value=context), \
                     patch.object(handles, "_group_live_layout_context", return_value=context):
                    self.assertIs(value.scale(2, about_point=(1, 3)), value)
                    self.assertIs(value.scale(.5, about_edge=noon.RIGHT), value)
                owner = context if live else anchor
                prefix = (anchor,) if live else ()
                self.assertEqual(getattr(owner, "liveScaleLayout" if live else "scale").call_args_list, [
                    unittest.mock.call(*prefix, 2., 2., 1., 3., True),
                    unittest.mock.call(*prefix, .5, .5, 1., 0., False)])

    def test_unknown_options_and_invalid_vector_shapes_do_not_dispatch(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch.object(handles, "_layout_anchor") as anchor:
            with self.assertRaises(NotImplementedError):
                value.rotate(1, surprise=True)
            with self.assertRaises(TypeError):
                value.flip((1, 2, 3, 4))
            anchor.assert_not_called()
