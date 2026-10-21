import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class FamilyIndexedQueryTests(unittest.TestCase):
    def group(self):
        values = [identity_only_wrapper(compat.Mobject) for _ in range(3)]
        for index, value in enumerate(values):
            value._semantic_handle = SimpleNamespace(semanticSlot=index, semanticGeneration=1)
        members = list(values[:2])

        class Bridge:
            semanticSlot, semanticGeneration = 99, 1
            memberKeys = Mock(side_effect=AssertionError("indexed read enumerated siblings"))

            @property
            def memberCount(self):
                return len(members)

        bridge = Bridge()
        bridge.splitCount = lambda inclusion=None: len(members)
        bridge.splitAt = Mock(side_effect=lambda index, inclusion=None: members[index]._semantic_handle if index < len(members) else None)
        group = identity_only_wrapper(compat.VGroup)
        group._semantic_family_handle = bridge
        group._semantic_member_wrappers = {f"{index}:1": value for index, value in enumerate(values)}
        return group, members, values, bridge

    def test_index_callback_reads_current_members_after_normalization(self):
        for operation in ("remove", "prepend"):
            with self.subTest(operation=operation):
                group, members, values, bridge = self.group()

                class Index:
                    def __index__(self):
                        if operation == "remove":
                            members.pop(0)
                        else:
                            members.insert(0, values[2])
                        return 0

                self.assertIs(group[Index()], values[1] if operation == "remove" else values[2])
                bridge.splitAt.assert_called_once_with(0, None)
                bridge.memberKeys.assert_not_called()

    def test_negative_normalization_uses_the_count_before_the_callback(self):
        group, members, values, bridge = self.group()

        class Index:
            def __index__(self):
                members.append(values[2])
                return -1

        self.assertIs(group[Index()], values[1])
        bridge.splitAt.assert_called_once_with(1, None)
        bridge.memberKeys.assert_not_called()

    def test_missing_rank_after_a_callback_is_a_list_index_error(self):
        group, members, _, bridge = self.group()

        class Index:
            def __index__(self):
                members.clear()
                return 0

        with self.assertRaisesRegex(IndexError, "^list index out of range$"):
            group[Index()]
        bridge.splitAt.assert_called_once_with(0, None)

    def test_invalid_index_protocols_fail_before_reading_members(self):
        group, _, _, bridge = self.group()
        with self.assertRaisesRegex(TypeError, "range indices must be integers or slices"):
            group[1.5]
        with self.assertRaisesRegex(IndexError, "range object index out of range"):
            group[2]
        bridge.splitAt.assert_not_called()
        bridge.memberKeys.assert_not_called()

    def test_explicit_js_null_is_a_missing_rank(self):
        group, _, _, bridge = self.group()
        sentinel = object()
        bridge.splitAt.return_value = sentinel
        bridge.splitAt.side_effect = None
        with patch.object(handles, "_jsnull", sentinel):
            with self.assertRaisesRegex(IndexError, "^list index out of range$"):
                group[0]

    def test_specialized_metadata_is_refreshed_for_known_identities_after_indexing(self):
        group, _, values, bridge = self.group()
        events = []

        class Index:
            def __index__(self):
                events.append("index")
                return 0

        bridge.splitAt.side_effect = lambda index, inclusion=None: (events.append("select"), values[index]._semantic_handle)[1]
        group._refresh_semantic_members = Mock(side_effect=lambda: events.append("refresh"))
        self.assertIs(group[Index()], values[0])
        self.assertEqual(events, ["index", "select", "refresh"])
        bridge.memberKeys.assert_not_called()

    def test_iteration_captures_native_cursor_eagerly_and_releases_it_once(self):
        group, _, values, bridge = self.group()
        selected = values[0]._semantic_handle
        selected.free = Mock()
        cursor = SimpleNamespace(nextMember=Mock(side_effect=[selected, None]), free=Mock())
        bridge.splitIterator = Mock(return_value=cursor)
        iterator = iter(group)
        bridge.splitIterator.assert_called_once_with(None)
        cursor.nextMember.assert_not_called()
        self.assertIs(iter(iterator), iterator)
        self.assertIs(next(iterator), values[0])
        selected.free.assert_called_once_with()
        scope = group._semantic_member_reference_scope
        self.assertEqual(scope.readers, 1)
        for _ in range(2):
            with self.assertRaises(StopIteration):
                next(iterator)
        self.assertEqual(scope.readers, 0)
        self.assertEqual(cursor.nextMember.call_count, 2)
        cursor.free.assert_called_once_with()

    def test_retained_cursor_resolves_original_wrappers_after_reference_fork(self):
        group, _, values, bridge = self.group()
        cursor = SimpleNamespace(nextMember=Mock(side_effect=[values[0]._semantic_handle, None]), free=Mock())
        bridge.splitIterator = Mock(return_value=cursor)
        iterator = iter(group)
        original = group._semantic_member_wrappers
        handles._fork_group_wrapper_references(group)
        self.assertIsNot(group._semantic_member_wrappers, original)
        group._semantic_member_wrappers.pop("0:1")
        self.assertIs(next(iterator), values[0])
        iterator.close()
        cursor.free.assert_called_once_with()

    def test_cursor_error_releases_native_and_host_references(self):
        group, _, _, bridge = self.group()
        cursor = SimpleNamespace(nextMember=Mock(side_effect=ValueError("cursor failed")), free=Mock())
        bridge.splitIterator = Mock(return_value=cursor)
        iterator = iter(group)
        with self.assertRaisesRegex(ValueError, "cursor failed"):
            next(iterator)
        self.assertEqual(group._semantic_member_reference_scope.readers, 0)
        self.assertIsNone(iterator._root)
        cursor.free.assert_called_once_with()
        with self.assertRaises(StopIteration):
            next(iterator)

    def test_custom_point_predicates_are_python_hooks_over_native_selection(self):
        group, _, _, bridge = self.group()
        group.has_points = Mock(return_value=True)
        bridge.splitCount = Mock(return_value=3)
        self.assertEqual(len(group), 3)
        bridge.splitCount.assert_called_once_with(True)
        group.has_no_points = Mock(return_value=False)
        bridge.splitAt.return_value = bridge
        bridge.splitAt.side_effect = None
        self.assertIs(group[0], group)
        bridge.splitAt.assert_called_once_with(0, True)
        group.has_no_points.assert_called_once_with()

    def test_length_dispatches_an_overridden_split(self):
        group, _, values, bridge = self.group()
        group.split = Mock(return_value=[group, *values])
        bridge.splitCount = Mock(side_effect=AssertionError("custom split was bypassed"))
        self.assertEqual(len(group), 4)
        group.split.assert_called_once_with()
        bridge.splitCount.assert_not_called()

    def test_copy_keeps_iterator_reference_ownership_on_the_source(self):
        from _typed_family_test_support import FakeMembershipBatch

        group, _, values, bridge = self.group()
        bridge.memberKeys = lambda: ["0:1", "1:1"]
        cursor = SimpleNamespace(nextMember=Mock(side_effect=[values[0]._semantic_handle, None]), free=Mock())
        bridge.splitIterator = Mock(return_value=cursor)
        iterator = iter(group)
        scope = group._semantic_member_reference_scope
        copied = SimpleNamespace(
            familyFor=lambda original: SimpleNamespace(semanticSlot=199, semanticGeneration=1),
            mobjectFor=lambda original: SimpleNamespace(semanticSlot=original.semanticSlot + 100, semanticGeneration=1),
        )
        bridge.copyFamily = Mock(return_value=copied)
        with patch.object(handles, "_new_membership_batch", FakeMembershipBatch):
            clone = group.copy()
        self.assertNotIn("_semantic_member_reference_scope", clone.__dict__)
        self.assertIs(group._semantic_member_reference_scope, scope)
        self.assertEqual(scope.readers, 1)
        self.assertEqual(set(clone._semantic_member_wrappers), {"100:1", "101:1"})
        self.assertTrue(all(value is not original for value, original in zip(clone._semantic_member_wrappers.values(), values)))
        self.assertIs(next(iterator), values[0])
        iterator.close()
        self.assertEqual(scope.readers, 0)
