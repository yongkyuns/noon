"""Grid frontend argument forwarding; shared Rust tests qualify layout semantics."""
import unittest
from unittest.mock import Mock, patch

import _manim_compat as compat

import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class SharedFamilyGridTests(unittest.TestCase):
    def test_one_authored_or_live_call_without_frontend_bounds(self):
        for live in (False, True):
            with self.subTest(live=live):
                family = identity_only_wrapper(compat.VGroup, submobjects=[])
                family._semantic_family_handle = Mock()
                context = Mock() if live else None
                family.get_center = Mock(side_effect=AssertionError("Rust owns grid bounds"))
                with patch.object(handles, "_group_live_layout_context", return_value=context):
                    self.assertIs(family.arrange_in_grid(rows=2, buff=(0.5, 0.25)), family)
                if live:
                    context.liveArrangeFamilyInGrid.assert_called_once_with(family._semantic_family_handle, 2, None, 0.5, 0.25)
                else:
                    family._semantic_family_handle.arrangeInGrid.assert_called_once_with(2, None, 0.5, 0.25)

    def test_invalid_dimensions_and_unbacked_family_fail_before_dispatch(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        for value in (0, -1, 2**32):
            with self.assertRaises(ValueError):
                family.arrange_in_grid(rows=value)
        with self.assertRaises(TypeError):
            family.arrange_in_grid(rows=1.5)
        with self.assertRaisesRegex(RuntimeError, "shared Rust"):
            family.arrange_in_grid()
