"""Argument/dispatch checks; Rust tests own alias, layout and atomicity semantics."""
import unittest
from unittest.mock import Mock, patch

import _manim_compat as compat
compat.install()
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class SharedFamilyAffineTests(unittest.TestCase):
    def test_authored_and_live_calls_forward_one_family_handle_without_python_layout(self):
        for live in (False, True):
            with self.subTest(live=live):
                family = identity_only_wrapper(compat.VGroup, submobjects=[])
                family._semantic_family_handle = Mock()
                family.get_center = Mock(side_effect=AssertionError("Python must not resolve family pivots"))
                context = Mock() if live else None
                with patch.object(handles, "_group_live_layout_context", return_value=context):
                    self.assertIs(family.scale((2, 3)), family)
                    self.assertIs(family.rotate(0.5, about_edge=(1, -1)), family)
                    self.assertIs(family.rotate(0.25, about_point=(4, 5)), family)
                owner = context if live else family._semantic_family_handle
                prefix = (family._semantic_family_handle,) if live else ()
                getattr(owner, "liveScaleFamily" if live else "scale").assert_called_once_with(*prefix, 2.0, 3.0)
                self.assertEqual(getattr(owner, "liveRotateFamily" if live else "rotate").call_args_list,
                                 [unittest.mock.call(*prefix, 0.5, 1.0, -1.0, False),
                                  unittest.mock.call(*prefix, 0.25, 4.0, 5.0, True)])

    def test_family_without_shared_handle_cannot_mutate(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        with self.assertRaisesRegex(RuntimeError, "shared Rust"):
            family.scale(2)
        with self.assertRaisesRegex(RuntimeError, "shared Rust"):
            family.rotate(0.5)
