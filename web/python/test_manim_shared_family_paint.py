"""Frontend coercion/dispatch; Rust tests own family paint semantics and atomicity."""
import unittest
from unittest.mock import Mock, patch

import _manim_compat as compat
compat.install()
import _manim_semantic_handles as handles
import _manim_updaters as updaters
from _typed_geometry_test_support import identity_only_wrapper


class SharedFamilyPaintTests(unittest.TestCase):
    def test_authored_and_live_calls_forward_one_family_handle(self):
        for live in (False, True):
            with self.subTest(live=live):
                family = identity_only_wrapper(compat.VGroup, submobjects=[])
                family._semantic_family_handle = Mock()
                context = Mock() if live else None
                with patch.object(handles, "_group_target_context", return_value=context):
                    self.assertIs(family.set_color(compat._base.RED), family)
                    self.assertIs(family.set_fill(compat._base.BLUE, 0.4), family)
                    self.assertIs(family.set_stroke(compat._base.RED, 2, 0.25), family)
                    self.assertIs(family.set_opacity(0.5), family)
                owner = context if live else family._semantic_family_handle
                prefix = (family._semantic_family_handle,) if live else ()
                def method(name):
                    return getattr(owner, f"liveSetFamily{name}" if live else f"set{name}")
                color = compat._base.RED
                method("Color").assert_called_once_with(*prefix, color.red, color.green, color.blue, color.alpha)
                color = compat._base.BLUE
                method("Fill").assert_called_once_with(*prefix, True, color.red, color.green, color.blue, color.alpha, 0.4)
                color = compat._base.RED
                method("Stroke").assert_called_once_with(*prefix, True, color.red, color.green, color.blue, color.alpha, compat._manim_stroke_width(2), 0.25)
                method("Opacity").assert_called_once_with(*prefix, 0.5)

    def test_callback_paint_keeps_using_effective_overlay(self):
        member = identity_only_wrapper(compat.Square)
        member.set_fill = Mock()
        family = identity_only_wrapper(compat.VGroup, submobjects=[member])
        family._semantic_family_handle = Mock()
        with patch.object(updaters, "_canonical_phase_context", return_value=Mock()):
            family.set_fill(compat._base.RED, 0.5)
        member.set_fill.assert_called_once_with(compat._base.RED, 0.5)
        family._semantic_family_handle.setFill.assert_not_called()

    def test_unbacked_family_cannot_mutate(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        with self.assertRaisesRegex(RuntimeError, "shared Rust"):
            family.set_opacity(0.5)
