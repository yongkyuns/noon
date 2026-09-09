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
