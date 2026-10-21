import unittest
from types import SimpleNamespace
from unittest.mock import Mock

import noon
import _manim_compat as compat
from _typed_geometry_test_support import identity_only_wrapper


class ObjectMemberQueryTests(unittest.TestCase):
    def root(self, cls=noon.Mobject):
        root = identity_only_wrapper(cls)
        alias = SimpleNamespace(semanticSlot=99, semanticGeneration=1, free=Mock())
        handle = SimpleNamespace(semanticSlot=99, semanticGeneration=1,
                                 splitCount=Mock(return_value=1), splitKeys=Mock(return_value=["99:1"]),
                                 splitAt=Mock(return_value=alias))
        root._semantic_handle = handle
        root._semantic_handle_fresh = True
        return root, handle, alias

    def test_ordinary_root_queries_preserve_the_existing_wrapper(self):
        for cls in (noon.Mobject, compat.VMobject, compat.Circle, compat.Square):
            with self.subTest(cls=cls):
                root, handle, alias = self.root(cls)
                self.assertEqual(len(root), 1)
                self.assertEqual(root.split(), [root])
                self.assertIs(root[0], root)
                handle.splitCount.assert_called_with(None)
                handle.splitKeys.assert_called_once_with(None)
                handle.splitAt.assert_called_once_with(0, None)
                alias.free.assert_called_once_with()

    def test_iterator_uses_object_capability_and_releases_reader_ownership(self):
        root, handle, alias = self.root()
        cursor = SimpleNamespace(nextMember=Mock(side_effect=[alias, None]), free=Mock())
        handle.splitIterator = Mock(return_value=cursor)
        iterator = iter(root)
        handle.splitIterator.assert_called_once_with(None)
        self.assertIs(next(iterator), root)
        with self.assertRaises(StopIteration): next(iterator)
        self.assertEqual(root._semantic_member_reference_scope.readers, 0)
        cursor.free.assert_called_once_with()

    def test_scalar_normalization_runs_before_current_point_selection(self):
        root, handle, _ = self.root()
        class Index:
            def __index__(self):
                handle.splitAt.return_value = None
                handle.splitCount.return_value = 0
                return -1
        with self.assertRaisesRegex(IndexError, "^list index out of range$"):
            root[Index()]
        handle.splitAt.assert_called_once_with(0, None)

    def test_slice_dispatches_class_hook_and_native_object_plan(self):
        root, handle, _ = self.root(compat.Circle)
        root.get_group_class = Mock(return_value=lambda *members: tuple(members))
        plan = SimpleNamespace(includesSelf=True, selectNormalized=Mock(return_value=["99:1"]), free=Mock())
        handle.prepareSlice = Mock(return_value=plan)
        self.assertEqual(root[:], (root,))
        root.get_group_class.assert_called_once_with()
        handle.prepareSlice.assert_called_once_with(None)
        plan.selectNormalized.assert_called_once_with(0., 1., 1.)
        plan.free.assert_called_once_with()

    def test_object_copy_excludes_source_iterator_reference_metadata(self):
        root, handle, alias = self.root(compat.Circle)
        cursor = SimpleNamespace(nextMember=Mock(side_effect=[alias, None]), free=Mock())
        handle.splitIterator = Mock(return_value=cursor)
        handle.cloneHandle = Mock(return_value=SimpleNamespace(semanticSlot=100, semanticGeneration=1))
        iterator = iter(root)
        scope = root._semantic_member_reference_scope
        cloned = root.copy()
        self.assertNotIn("_semantic_member_reference_scope", cloned.__dict__)
        self.assertEqual(scope.readers, 1)
        self.assertIs(next(iterator), root)
        iterator.close()
        self.assertEqual(scope.readers, 0)
