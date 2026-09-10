"""Frontend coercion/dispatch; Rust tests own family paint semantics and atomicity."""
import unittest
from unittest.mock import Mock, patch

import _manim_compat as compat

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
        phase = Mock()
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(phase)
        try:
            family.set_fill(compat._base.RED, 0.5)
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        color = compat._base.RED
        phase.paint_family.assert_called_once_with(family._semantic_family_handle, "Fill",
            (True, color.red, color.green, color.blue, color.alpha, 0.5))
        member.set_fill.assert_not_called()
        family._semantic_family_handle.setFill.assert_not_called()

    def test_unbacked_family_cannot_mutate(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        with self.assertRaisesRegex(RuntimeError, "shared Rust"):
            family.set_opacity(0.5)


class FamilyCallbackBatchTests(unittest.TestCase):
    def fixture(self):
        import copy
        from types import SimpleNamespace
        from test_updater_snapshot import CanonicalCallbackPropertyRowTests
        _, mobject, context = CanonicalCallbackPropertyRowTests._mobject_and_context()
        context.token["publication"]["scene_revision"] = "9"
        _, row = context.row(mobject)
        second = copy.deepcopy(context._frame_items[(11, 3)])
        second["node"] = {"slot": 12, "generation": 3}
        context._operations.callbackFamilyKeys = Mock(return_value=["11:3", "12:3"])
        context._operations.callbackFamilyPaint = Mock()
        family = SimpleNamespace(semanticSlot=21, semanticGeneration=3)
        return context, row, second, family

    def test_sparse_family_is_one_read_and_preserves_preceding_overlay_writes(self):
        import json
        from dataclasses import replace
        context, row, second, family = self.fixture()
        row.style = replace(row.style, opacity=0.25)
        context._read = Mock(return_value={"kind": "family", "objects": [context._frame_items[(11, 3)], second]})
        # No changes returned by the Rust operation: verify adaptation, not paint math.
        context._operations.callbackFamilyPaint.return_value = "[]"
        context.paint_family(family, "Opacity", (0.5,))
        context._read.assert_called_once_with("family", (21, 3))
        forwarded = json.loads(context._operations.callbackFamilyPaint.call_args.args[3])
        self.assertEqual(forwarded[0][2]["opacity"], 0.25)
        self.assertEqual(context.effective_batch()["writes"], [])
        context._read.reset_mock()
        context.paint_family(family, "Opacity", (0.5,))
        context._read.assert_not_called()

    def test_failed_bulk_read_or_late_result_decode_cannot_publish_partial_edits(self):
        import json
        context, row, second, family = self.fixture()
        before = row.style
        context._read = Mock(side_effect=RuntimeError("last member is not live"))
        with self.assertRaisesRegex(RuntimeError, "last member"):
            context.paint_family(family, "Opacity", (0.5,))
        context._operations.callbackFamilyPaint.assert_not_called()
        self.assertEqual(row.style, before)
        self.assertEqual(context.effective_batch()["writes"], [])
        context._read = Mock(return_value={"kind": "family", "objects": [context._frame_items[(11, 3)], second]})
        changed = {**before.to_wire(), "opacity": 0.5}
        context._operations.callbackFamilyPaint.return_value = json.dumps([[11, 3, changed], [12, 3, {}]])
        with self.assertRaises(TypeError):
            context.paint_family(family, "Opacity", (0.5,))
        self.assertEqual(row.style, before)
        self.assertEqual(context.effective_batch()["writes"], [])
        self.assertNotIn((12, 3), context._rows)


    def test_family_recolor_preserves_bounds_but_stroke_width_invalidates_them(self):
        import json
        context, row, second, family = self.fixture()
        context._read = Mock(return_value={"kind": "family", "objects": [context._frame_items[(11, 3)], second]})
        bounds = row.require_bounds()
        recolored = row.style.to_wire()
        recolored["stroke"] = {"red": 0.2, "green": 0.4, "blue": 1.0, "alpha": 0.6}
        context._operations.callbackFamilyPaint.return_value = json.dumps([[11, 3, recolored]])
        context.paint_family(family, "Color", (0.2, 0.4, 1.0, 0.6))
        self.assertEqual(row.require_bounds(), bounds)
        wider = {**recolored, "stroke_width": 3.0}
        context._operations.callbackFamilyPaint.return_value = json.dumps([[11, 3, wider]])
        context.paint_family(family, "Stroke", (False, 0, 0, 0, 1, 3.0, None))
        with self.assertRaisesRegex(NotImplementedError, "spatial property change"):
            row.require_bounds()
