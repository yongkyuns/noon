"""Thin priority dispatch; ordering and rollback are qualified in shared Rust."""
import unittest
from unittest.mock import Mock, patch, call
import _manim_compat as compat
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class ZIndexTests(unittest.TestCase):
    def test_leaf_and_family_use_shared_authored_or_live_dispatch(self):
        for cls in (compat.VMobject, compat.VGroup):
            for live in (False, True):
                value = identity_only_wrapper(cls, submobjects=[])
                anchor, context = Mock(), (Mock() if live else None)
                anchor.zIndex.return_value = -2.5
                with patch.object(handles, "_layout_anchor", return_value=anchor), \
                     patch.object(handles, "_live_mutation_context", return_value=context), \
                     patch.object(handles, "_group_live_layout_context", return_value=context):
                    self.assertEqual(value.z_index, -2.5)
                    self.assertIs(value.set_z_index(3.25), value)
                    value.z_index = -1.5
                owner = context if live else anchor
                prefix = (anchor,) if live else ()
                self.assertEqual(getattr(owner, "liveSetZIndex" if live else "setZIndex").call_args_list,
                                 [call(*prefix, 3.25, True), call(*prefix, -1.5, False)])
                self.assertNotIn("z_index", value.__dict__)

    def test_phase_edits_reject_before_dispatch(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch("_manim_updaters._canonical_phase_context", return_value=Mock()), \
             patch.object(handles, "_layout_anchor") as anchor:
            with self.assertRaises(NotImplementedError):
                value.set_z_index(1)
            anchor.assert_not_called()
