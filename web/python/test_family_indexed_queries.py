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
            memberKeys = Mock(side_effect=AssertionError("indexed read enumerated siblings"))

            @property
            def memberCount(self):
                return len(members)

        bridge = Bridge()
        bridge.memberAt = Mock(side_effect=lambda index: members[index]._semantic_handle if index < len(members) else None)
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
                bridge.memberAt.assert_called_once_with(0)
                bridge.memberKeys.assert_not_called()

    def test_negative_normalization_uses_the_count_before_the_callback(self):
        group, members, values, bridge = self.group()

        class Index:
            def __index__(self):
                members.append(values[2])
                return -1

        self.assertIs(group[Index()], values[1])
        bridge.memberAt.assert_called_once_with(1)
        bridge.memberKeys.assert_not_called()

    def test_missing_rank_after_a_callback_is_a_list_index_error(self):
        group, members, _, bridge = self.group()

        class Index:
            def __index__(self):
                members.clear()
                return 0

        with self.assertRaisesRegex(IndexError, "^list index out of range$"):
            group[Index()]
        bridge.memberAt.assert_called_once_with(0)

    def test_invalid_index_protocols_fail_before_reading_members(self):
        group, _, _, bridge = self.group()
        with self.assertRaisesRegex(TypeError, "range indices must be integers or slices"):
            group[1.5]
        with self.assertRaisesRegex(IndexError, "range object index out of range"):
            group[2]
        bridge.memberAt.assert_not_called()
        bridge.memberKeys.assert_not_called()

    def test_explicit_js_null_is_a_missing_rank(self):
        group, _, _, bridge = self.group()
        sentinel = object()
        bridge.memberAt.return_value = sentinel
        bridge.memberAt.side_effect = None
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

        bridge.memberAt.side_effect = lambda index: (events.append("select"), values[index]._semantic_handle)[1]
        group._refresh_semantic_members = Mock(side_effect=lambda: events.append("refresh"))
        self.assertIs(group[Index()], values[0])
        self.assertEqual(events, ["index", "select", "refresh"])
        bridge.memberKeys.assert_not_called()
