import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class FamilyMembershipOrderTests(unittest.TestCase):
    def test_mutations_update_identity_references_without_querying_sibling_order(self):
        a = identity_only_wrapper(compat.Mobject)
        b = identity_only_wrapper(compat.Mobject)
        a._semantic_handle = SimpleNamespace(semanticSlot=1, semanticGeneration=0)
        b._semantic_handle = SimpleNamespace(semanticSlot=2, semanticGeneration=0)
        bridge = SimpleNamespace(memberCount=2, memberKeys=Mock(return_value=['2:0', '1:0']),
                                 editMembership=Mock(return_value=[False]))
        group = identity_only_wrapper(compat.Group)
        group._semantic_family_handle = bridge
        group._semantic_member_wrappers = {'1:0': a, '2:0': b}
        with patch.object(handles, '_family_membership_batch', return_value=object()), \
             patch.object(handles, '_live_constructor_context', return_value=None):
            self.assertIs(group.add(a), group)
            bridge.memberKeys.assert_not_called()
            self.assertEqual(len(group), 2)
            bridge.memberKeys.assert_not_called()
            self.assertEqual(group.submobjects, [b, a])
            bridge.memberKeys.reset_mock()
            bridge.editMembership.return_value = [True]
            self.assertIs(group.remove(a), group)
            bridge.memberKeys.assert_not_called()
            self.assertEqual(group._semantic_member_wrappers, {'2:0': b})
            bridge.editMembership.side_effect = RuntimeError('rejected')
            with self.assertRaises(RuntimeError):
                group.add(a)
            self.assertEqual(group._semantic_member_wrappers, {'2:0': b})

    def test_live_route_uses_the_same_rust_order_and_preserves_wrapper_identity(self):
        a = identity_only_wrapper(compat.Mobject)
        a._semantic_handle = SimpleNamespace(semanticSlot=7, semanticGeneration=3)
        bridge = SimpleNamespace(memberCount=1, memberKeys=Mock(return_value=['7:3']))
        group = identity_only_wrapper(compat.VGroup)
        group._semantic_family_handle = bridge
        group._semantic_member_wrappers = {}
        context = SimpleNamespace(liveEditFamilyMembership=Mock(return_value=[True]))
        with patch.object(handles, '_family_membership_batch', return_value='batch'), \
             patch.object(handles, '_live_constructor_context', return_value=context):
            group.add(a)
        context.liveEditFamilyMembership.assert_called_once_with(bridge, 'batch')
        self.assertIs(group[0], a)
