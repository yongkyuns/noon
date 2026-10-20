import unittest
from types import SimpleNamespace
from unittest.mock import Mock, PropertyMock, patch

import _manim_semantic_handles as handles
from _manim_compat import Group, VGroup


class AddNMoreSubmobjectsTests(unittest.TestCase):
    def make_family(self):
        value = object.__new__(Group)
        family = SimpleNamespace(prependMembers=Mock())
        value._semantic_family_handle = family
        value._semantic_member_wrappers = {}
        return value, family

    def test_adds_faded_copies_in_manim_repeat_order_with_one_rust_edit(self):
        value, family = self.make_family()
        first, second = Mock(), Mock()
        copies = [Mock(), Mock(), Mock()]
        first.copy.side_effect = copies[:2]
        second.copy.return_value = copies[2]
        batch = object()
        members = [first, second]

        with (
            patch.object(Group, "submobjects", new_callable=PropertyMock) as children,
            patch.object(handles, "_live_constructor_context", return_value=None),
            patch.object(handles, "_family_membership_batch", return_value=batch) as make_batch,
            patch.object(handles, "_family_wrapper_key", side_effect=lambda item: str(id(item))),
            patch.object(handles, "engine_call", side_effect=lambda fn, *args, **kw: fn(*args, **kw)),
        ):
            children.return_value = members
            self.assertIs(Group.add_n_more_submobjects(value, 3), value)

        expected = [first, copies[0], copies[1], second, copies[2]]
        make_batch.assert_called_once_with(None, "add", tuple(expected))
        family.prependMembers.assert_called_once_with(
            batch, operation="Mobject.add_n_more_submobjects"
        )
        self.assertEqual([copies[0].fade.call_args, copies[1].fade.call_args, copies[2].fade.call_args], [
            ((1,),), ((1,),), ((1,),),
        ])
        self.assertEqual(set(value._semantic_member_wrappers.values()), set(expected))

    def test_zero_and_negative_requests_keep_the_source_return_contract(self):
        value, _family = self.make_family()
        first = Mock()
        with patch.object(Group, "submobjects", new_callable=PropertyMock) as children:
            children.return_value = [first]
            self.assertIsNone(Group.add_n_more_submobjects(value, 0))
            self.assertIs(Group.add_n_more_submobjects(value, -1), value)
        first.copy.assert_not_called()

    def test_empty_vgroup_uses_point_mobjects_and_returns_none(self):
        value = object.__new__(VGroup)
        family = SimpleNamespace(prependMembers=Mock())
        value._semantic_family_handle = family
        value._semantic_member_wrappers = {}
        points = [Mock(), Mock()]
        value.get_point_mobject = Mock(side_effect=points)
        batch = object()

        with (
            patch.object(Group, "submobjects", new_callable=PropertyMock) as children,
            patch.object(handles, "_live_constructor_context", return_value=None),
            patch.object(handles, "_family_membership_batch", return_value=batch) as make_batch,
            patch.object(handles, "_family_wrapper_key", side_effect=lambda item: str(id(item))),
            patch.object(handles, "engine_call", side_effect=lambda fn, *args, **kw: fn(*args, **kw)),
        ):
            children.return_value = []
            self.assertIsNone(VGroup.add_n_more_submobjects(value, 2))

        self.assertEqual(value.get_point_mobject.call_count, 2)
        make_batch.assert_called_once_with(None, "add", tuple(points))
        family.prependMembers.assert_called_once_with(
            batch, operation="Mobject.add_n_more_submobjects"
        )

    def test_empty_vgroup_point_mobject_matches_only_the_vector_root_style(self):
        value = object.__new__(VGroup)
        point = Mock()
        with patch("_manim_compat.VectorizedPoint", return_value=point), patch.object(
            VGroup, "get_center", return_value=(2.0, 3.0)
        ):
            result = VGroup.get_point_mobject(value)
        self.assertIs(result, point)
        point.match_style.assert_called_once_with(value, family=False)

    def test_empty_plain_group_preserves_manims_point_override_error(self):
        value, _family = self.make_family()
        with patch.object(Group, "submobjects", new_callable=PropertyMock) as children:
            children.return_value = []
            with self.assertRaisesRegex(NotImplementedError, "get_point_mobject not implemented"):
                Group.add_n_more_submobjects(value, 1)


if __name__ == "__main__":
    unittest.main()
