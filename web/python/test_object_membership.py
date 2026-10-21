"""Forwarding tests use canned native decisions; membership remains Rust-owned."""
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import noon
import _manim_compat as compat
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class ObjectMembershipTests(unittest.TestCase):
    def object(self, slot):
        value = identity_only_wrapper(compat.Circle)
        value._semantic_handle = SimpleNamespace(semanticSlot=slot, semanticGeneration=1, memberKeys=Mock(return_value=[]))
        value._semantic_handle_fresh = True
        return value

    def test_ordinary_edits_forward_typed_batches_and_keep_existing_wrappers(self):
        root, child = self.object(10), self.object(11)
        root._semantic_handle.editMembership = Mock(return_value=[1])
        root._semantic_handle.prependMembers = Mock()
        root._semantic_handle.replaceMembers = Mock()
        root._semantic_handle.memberKeys.return_value = ['11:1']
        batch = object()
        with patch.object(handles, '_live_constructor_context', return_value=None), patch.object(handles, '_family_membership_batch', return_value=batch):
            self.assertIs(root.add(child), root)
            self.assertEqual(root.submobjects, [child])
            self.assertIs(root.add_to_back(child), root)
            root.submobjects = [child]
            root.remove(child)
        root._semantic_handle.editMembership.assert_any_call(batch)
        root._semantic_handle.prependMembers.assert_called_once_with(batch)
        root._semantic_handle.replaceMembers.assert_called_once_with(batch)
        self.assertEqual(root._semantic_member_wrappers, {})

    def test_rejected_edit_does_not_publish_host_identity_references(self):
        root, child = self.object(10), self.object(11)
        root._semantic_handle.editMembership = Mock(side_effect=RuntimeError('rejected Rust transaction'))
        with patch.object(handles, '_live_constructor_context', return_value=None), patch.object(handles, '_family_membership_batch', return_value=object()):
            with self.assertRaisesRegex(RuntimeError, 'rejected Rust transaction'):
                root.add(child)
        self.assertNotIn('_semantic_member_wrappers', root.__dict__)

    def test_live_object_edits_choose_object_bindings(self):
        root, child = self.object(10), self.object(11)
        context = SimpleNamespace(liveEditObjectMembership=Mock(return_value=[1]), livePrependObjectMembers=Mock(), liveReplaceObjectMembers=Mock())
        batch = object()
        with patch.object(handles, '_live_constructor_context', return_value=context), patch.object(handles, '_family_membership_batch', return_value=batch):
            root.add(child)
            root.add_to_back(child)
            root.submobjects = [child]
        context.liveEditObjectMembership.assert_called_once_with(root._semantic_handle, batch)
        context.livePrependObjectMembers.assert_called_once_with(root._semantic_handle, batch)
        context.liveReplaceObjectMembers.assert_called_once_with(root._semantic_handle, batch)

    def test_invalid_members_reject_before_building_native_batch(self):
        root = self.object(10)
        with patch.object(handles, '_family_membership_batch') as batch:
            with self.assertRaisesRegex(ValueError, 'Cannot add Circle as a submobject of itself'):
                root.add(root)
            with self.assertRaisesRegex(TypeError, 'Only values of type VMobject'):
                root.add(3)
            batch.assert_not_called()

    def test_object_copy_uses_the_shared_map_for_children_and_metadata_aliases(self):
        root, child = self.object(10), self.object(11)
        root._semantic_member_wrappers = {'11:1': child}
        root._semantic_handle.memberKeys.return_value = ['11:1']
        root.aliases = [child, child]
        copied_handles = {10: SimpleNamespace(semanticSlot=20, semanticGeneration=1, memberKeys=Mock(return_value=['21:1'])), 11: SimpleNamespace(semanticSlot=21, semanticGeneration=1, memberKeys=Mock(return_value=[]))}
        copied = SimpleNamespace(mobjectFor=Mock(side_effect=lambda source: copied_handles[source.semanticSlot]))
        root._semantic_handle.copyFamily = Mock(return_value=copied)
        with patch.object(handles, '_group_target_context', return_value=None), patch.object(handles, '_family_membership_batch', return_value=object()):
            clone = root.copy()
        self.assertIsNot(clone, root)
        self.assertIs(clone.aliases[0], clone.aliases[1])
        self.assertIs(clone.aliases[0], clone.submobjects[0])
        self.assertIsNot(clone.submobjects[0], child)
        self.assertEqual(clone._semantic_handle.semanticSlot, 20)
