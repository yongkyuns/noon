"""Typed forwarding and host wrapper identities; no semantic engine double."""
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import noon
import _manim_compat as compat
import _manim_point_promotion as protocol
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class PointPromotionBridgeTests(unittest.TestCase):
    def test_default_public_classes_select_one_shared_promotion(self):
        for cls in (noon.Mobject, compat.VMobject, compat.Circle, compat.Square,
                    compat.VectorizedPoint, compat.Group, compat.VGroup):
            with self.subTest(cls=cls.__name__):
                root = identity_only_wrapper(cls)
                self.assertTrue(protocol._uses_default_protocol(root))
                with patch.object(handles, '_group_copy_operation') as copy:
                    self.assertIs(root.push_self_into_submobjects(), root)
                copy.assert_called_once_with(root, cyclic_replace=False, promote_own_points=True)

    def test_custom_protocol_members_do_not_use_the_atomic_default(self):
        for name in ('copy', 'add', 'reset_points', '_assert_valid_submobjects',
                     '_assert_valid_submobjects_internal'):
            with self.subTest(name=name):
                root = identity_only_wrapper(noon.Mobject)
                setattr(root, name, Mock())
                self.assertFalse(protocol._uses_default_protocol(root))

    def test_copy_map_rebinding_and_reader_references_are_shared_for_all_typed_hosts(self):
        for family in (False, True):
            for live in (False, True):
                with self.subTest(family=family, live=live):
                    cls = compat.Group if family else noon.Mobject
                    source, clone = identity_only_wrapper(cls), identity_only_wrapper(cls)
                    child, copied_child = identity_only_wrapper(noon.Mobject), identity_only_wrapper(noon.Mobject)
                    original = SimpleNamespace(semanticSlot=1, semanticGeneration=0)
                    copied_root = SimpleNamespace(semanticSlot=3, semanticGeneration=0)
                    child._semantic_handle = SimpleNamespace(semanticSlot=2, semanticGeneration=0)
                    child._semantic_handle_fresh = True
                    copied_leaf = SimpleNamespace(semanticSlot=4, semanticGeneration=0)
                    attribute = '_semantic_family_handle' if family else '_semantic_handle'
                    setattr(source, attribute, original)
                    source._semantic_handle_fresh = True
                    source._semantic_member_wrappers = {'2:0': child}
                    clone.alias = copied_child
                    scope = handles._retain_group_wrapper_references(source)
                    mapping = {id(original): copied_root, id(child._semantic_handle): copied_leaf}
                    lookup = Mock(side_effect=lambda value: mapping[id(value)])
                    copied = SimpleNamespace(mobjectFor=lookup, familyFor=lookup)
                    original.promoteOwnPoints = Mock(return_value=copied)
                    context = None
                    if live:
                        context = SimpleNamespace(
                            livePromoteFamilyOwnPoints=Mock(return_value=copied),
                            livePromoteMobjectOwnPoints=Mock(return_value=copied))
                    references = object()
                    prepared = (clone, [(source, clone), (child, copied_child)], [(clone, [copied_child])])
                    with patch.object(handles, '_group_target_context', return_value=context), \
                         patch.object(compat, 'prepare_family_wrapper_copy', return_value=prepared), \
                         patch.object(handles, '_family_membership_batch', return_value=references) as batch:
                        actual = handles._group_copy_operation(source, cyclic_replace=False, promote_own_points=True)
                    self.assertIs(actual, clone)
                    self.assertIs(clone.alias, copied_child)
                    self.assertIs(copied_child._semantic_handle, copied_leaf)
                    self.assertIs(getattr(clone, attribute), copied_root)
                    self.assertEqual(clone._semantic_member_wrappers, {})
                    self.assertEqual(source._semantic_member_wrappers, {'2:0': child, '3:0': clone})
                    self.assertEqual(scope.wrappers, {'2:0': child})
                    self.assertIsNot(scope.wrappers, source._semantic_member_wrappers)
                    batch.assert_called_once_with(context, 'add', (source, child))
                    if live:
                        method = context.livePromoteFamilyOwnPoints if family else context.livePromoteMobjectOwnPoints
                        method.assert_called_once_with(original, references)
                        original.promoteOwnPoints.assert_not_called()
                    else:
                        original.promoteOwnPoints.assert_called_once_with(references)

    def test_fallible_metadata_finishes_before_any_engine_promotion(self):
        root = identity_only_wrapper(noon.Mobject)
        original = SimpleNamespace(promoteOwnPoints=Mock())
        root._semantic_handle = original
        with patch.object(handles, '_group_target_context', return_value=None), \
             patch.object(compat, 'prepare_family_wrapper_copy', side_effect=LookupError('metadata refused')):
            with self.assertRaisesRegex(LookupError, 'metadata refused'):
                handles._group_copy_operation(root, cyclic_replace=False, promote_own_points=True)
        original.promoteOwnPoints.assert_not_called()


if __name__ == '__main__':
    unittest.main()
