"""Callback boundary adaptation only; shared Rust tests own translation semantics."""
import copy
import json
import unittest
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
import _manim_updaters as updaters
from _typed_geometry_test_support import identity_only_wrapper
import test_manim_shared_family_paint as paint_fixture


class CallbackFamilyTranslationTests(unittest.TestCase):
    def test_layout_replace_reads_and_stages_the_complete_rust_projection(self):
        context, row, second, family = paint_fixture.FamilyCallbackBatchTests().fixture()
        source, target = Mock(), Mock()
        source_anchor, target_anchor = Mock(), Mock()
        bounds = {"min": {"x": 1.0, "y": 0.0}, "max": {"x": 3.0, "y": 2.0}}
        context._frame_items[(11, 3)]["bounds"] = bounds
        target_row = {"transform": second["transform"], "style": second["style"],
                      "bounds": {"min": {"x": 5.0, "y": 0.0}, "max": {"x": 9.0, "y": 4.0}}}
        context._read = Mock(return_value={"kind": "object", "object": {
            **target_row, "node": {"slot": 21, "generation": 3}
        }})
        context._operations.callbackLayoutKeys = Mock(side_effect=[[11, 3], [21, 3]])
        context._operations.callbackLayoutReplace = Mock(
            return_value=[11, 3, 6.0, 0.0, 2.0, 1.0, 0.0, 1.0, 5.0, 1.0, 9.0, 3.0]
        )
        with patch("_manim_semantic_handles._layout_anchor",
                   side_effect=lambda value, **kwargs: source_anchor if value is source else target_anchor):
            context.replace_layout(source, target)
        context._operations.callbackLayoutReplace.assert_called_once()
        keys = context._operations.callbackLayoutReplace.call_args.args[3]
        wire = context._operations.callbackLayoutReplace.call_args.args[4]
        self.assertEqual(keys, [11, 3, 21, 3])
        self.assertEqual(len(wire), 24)
        self.assertEqual(row.transform.translation_x, 6.0)
        self.assertEqual(row.bounds, (5.0, 1.0, 9.0, 3.0))
        self.assertEqual(context.effective_batch()["writes"][-1]["kind"], "scale")

    def test_layout_replace_rejection_does_not_publish_partial_rows_or_writes(self):
        context, row, second, _ = paint_fixture.FamilyCallbackBatchTests().fixture()
        before = copy.deepcopy(row)
        source, target = Mock(), Mock()
        context._operations.callbackLayoutKeys = Mock(side_effect=[[11, 3], [21, 3]])
        context._read = Mock(return_value={"kind": "object", "object": {
            **second, "node": {"slot": 21, "generation": 3}
        }})
        context._operations.callbackLayoutReplace = Mock(side_effect=ValueError("bad Rust bounds"))
        with patch("_manim_semantic_handles._layout_anchor", side_effect=[Mock(), Mock()]):
            with self.assertRaisesRegex(ValueError, "bad Rust bounds"):
                context.replace_layout(source, target)
        self.assertEqual(row, before)
        self.assertEqual(context.effective_batch()["writes"], [])
        self.assertNotIn((21, 3), context._rows)

    def test_layout_replace_rejects_malformed_typed_result_before_staging(self):
        context, row, second, _ = paint_fixture.FamilyCallbackBatchTests().fixture()
        before = copy.deepcopy(row)
        context._operations.callbackLayoutKeys = Mock(side_effect=[[11, 3], [21, 3]])
        context._read = Mock(return_value={"kind": "object", "object": {
            **second, "node": {"slot": 21, "generation": 3}
        }})
        context._operations.callbackLayoutReplace = Mock(
            return_value=[11, 3, 6.0, 0.0, 2.0, 1.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0]
        )
        with patch("_manim_semantic_handles._layout_anchor", side_effect=[Mock(), Mock()]):
            with self.assertRaisesRegex(RuntimeError, "invalid typed layout row"):
                context.replace_layout(Mock(), Mock())
        self.assertEqual(row, before)
        self.assertEqual(context.effective_batch()["writes"], [])

    def test_mobject_replace_routes_through_the_active_callback_context(self):
        context, _, second, _ = paint_fixture.FamilyCallbackBatchTests().fixture()
        scene = compat._base.Scene()
        context._scene = scene
        source = identity_only_wrapper(compat.Square)
        target = identity_only_wrapper(compat.Square)
        source._scene = target._scene = scene
        source._object = target._object = object()
        source_anchor, target_anchor = Mock(), Mock()
        source._semantic_handle = Mock(
            semanticSlot=11, semanticGeneration=3,
            layoutAnchor=Mock(return_value=source_anchor),
        )
        target._semantic_handle = Mock(
            semanticSlot=12, semanticGeneration=3,
            layoutAnchor=Mock(return_value=target_anchor),
        )
        source._semantic_handle_fresh = target._semantic_handle_fresh = True
        context._frame_items[(12, 3)] = second
        context._operations.callbackLayoutKeys = Mock(side_effect=[[11, 3], [12, 3]])
        context._operations.callbackLayoutReplace = Mock(
            return_value=[11, 3, 6.0, 0.0, 2.0, 1.0, 0.0, 1.0, 5.0, 1.0, 9.0, 3.0]
        )
        with patch.dict(updaters._ACTIVE_CONTEXTS, {id(scene): context}):
            self.assertIs(handles._replace(source, target), source)
        context._operations.callbackLayoutReplace.assert_called_once()
        self.assertEqual(context.effective_batch()["writes"][-1]["kind"], "scale")

    def test_callback_layout_anchor_bypasses_only_its_own_phase_guard(self):
        scene = compat._base.Scene()
        value = identity_only_wrapper(compat.Square)
        value._scene = scene
        value._object = object()
        anchor = Mock()
        value._semantic_handle = Mock(layoutAnchor=Mock(return_value=anchor))
        value._semantic_handle_fresh = True
        phase = Mock(_scene=scene)
        with patch.object(updaters, "_canonical_phase_context", return_value=phase):
            with self.assertRaisesRegex(NotImplementedError, "active callback phase"):
                handles._layout_anchor(value)
            self.assertIs(handles._layout_anchor(value, callback_context=phase), anchor)
            with self.assertRaisesRegex(NotImplementedError, "active callback phase"):
                handles._layout_anchor(value, callback_context=Mock(_scene=scene))
        value._scene = compat._base.Scene()
        with patch.object(updaters, "_canonical_phase_context", return_value=None):
            with self.assertRaisesRegex(RuntimeError, "active Scene"):
                handles._layout_anchor(value, callback_context=phase)
        value._semantic_handle.layoutAnchor.assert_called_once()

    def test_group_forwards_one_typed_handle_without_walking_wrapper_members(self):
        member = identity_only_wrapper(compat.Circle)
        member.shift = Mock()
        family = identity_only_wrapper(compat.Group, submobjects=[member, member])
        family._semantic_family_handle = Mock()
        phase = Mock()
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(phase)
        try:
            with patch.object(handles, "_group_target_context", side_effect=AssertionError("wrapper walk")):
                self.assertIs(family.shift((1, -2)), family)
            phase.shift_family.assert_called_once_with(family._semantic_family_handle, compat._base.Vec2(1, -2))
            member.shift.assert_not_called()
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        self.assertFalse(hasattr(compat, "_shift_group_members"))

    def test_sparse_pinned_rows_retain_prior_overlay_and_returned_bounds(self):
        context, row, second, family = paint_fixture.FamilyCallbackBatchTests().fixture()
        context._read = Mock(return_value={"kind": "family", "objects": [context._frame_items[(11,3)], second]})
        before = row.transform
        row.shift(compat._base.Vec2(0.25, 0))
        context.transform_changed((11,3), before, row)
        prior = list(context.effective_batch()["writes"])
        translated = row.transform.to_wire()
        translated["translation"]["x"] = 4.25
        bounds = {"min": {"x": 3.25, "y": -2}, "max": {"x": 5.25, "y": 0}}
        context._operations.callbackFamilyShift = Mock(return_value=json.dumps([[11,3,translated,bounds]]))
        context.shift_family(family, compat._base.Vec2(2,0))
        forwarded=json.loads(context._operations.callbackFamilyShift.call_args.args[2])
        self.assertEqual(forwarded[0][2]["translation"]["x"],2.25)
        self.assertEqual(context.effective_batch()["writes"][:-1],prior)
        self.assertEqual(row.center(),compat._base.Vec2(4.25,-1))
        context._read.assert_called_once_with("family",(21,3))

    def test_read_preparation_and_late_decode_failures_leave_rows_and_writes_unchanged(self):
        for failure in ("read", "prepare", "decode"):
            with self.subTest(failure=failure):
                context,row,second,family=paint_fixture.FamilyCallbackBatchTests().fixture()
                before=copy.deepcopy(row)
                before_writes=list(context.effective_batch()["writes"])
                context._read=Mock(return_value={"kind":"family","objects":[context._frame_items[(11,3)],second]})
                prepare=Mock(return_value=json.dumps([[11,3,row.transform.to_wire(),None],[12,3,{},None]]))
                context._operations.callbackFamilyShift=prepare
                if failure=="read": context._read.side_effect=RuntimeError("late read")
                if failure=="prepare": prepare.side_effect=ValueError("late coordinate overflow")
                with self.assertRaises((RuntimeError,ValueError,TypeError)):
                    context.shift_family(family,compat._base.Vec2(1,0))
                self.assertEqual(row,before)
                self.assertEqual(context.effective_batch()["writes"],before_writes)
                self.assertNotIn((12,3),context._rows)
                if failure=="read": prepare.assert_not_called()

    def test_unbacked_group_rejects_without_a_member_fallback(self):
        member=identity_only_wrapper(compat.Circle)
        member.shift=Mock()
        family=identity_only_wrapper(compat.Group,submobjects=[member])
        token=updaters._ACTIVE_CANONICAL_CONTEXT.set(Mock())
        try:
            with self.assertRaisesRegex(RuntimeError,"shared Rust"):
                family.shift((1,0))
            member.shift.assert_not_called()
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
