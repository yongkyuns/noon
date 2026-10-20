"""Wrapper routing over Rust-owned root queries, independent of child traversal."""
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import noon
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class FamilyOwnPathTests(unittest.TestCase):
    def query(self, count):
        query = Mock(pointCount=count)
        query.anchors.return_value = [] if count == 0 else [1 + 2 ** -40, 0., 3., 2.]
        return query

    def test_authored_vector_root_observation_uses_one_native_query_and_no_children(self):
        for count in (0, 4):
            with self.subTest(count=count):
                value = identity_only_wrapper(noon.VGroup)
                query = self.query(count)
                family = SimpleNamespace(pathQuery=Mock(return_value=query), memberKeys=Mock(side_effect=AssertionError('child capture')))
                value._semantic_family_handle = family
                with patch.object(handles, '_live_mutation_context', return_value=None), patch.object(handles, '_live_constructor_context', return_value=None), patch.object(handles, '_group_target_context', side_effect=AssertionError('child context search')):
                    result = value.get_anchors()
                self.assertEqual([list(point) for point in result], [] if count == 0 else [[1 + 2 ** -40, 0.], [3., 2.]])
                family.pathQuery.assert_called_once_with()
                family.memberKeys.assert_not_called()
                query.free.assert_called_once_with()

    def test_effective_root_observation_reaches_the_current_owner_without_authored_fallback(self):
        value = identity_only_wrapper(noon.VGroup)
        query = self.query(4)
        family = SimpleNamespace(pathQuery=Mock(side_effect=AssertionError('stale authored query')))
        value._semantic_family_handle = family
        context = SimpleNamespace(queryFamilyOwnPath=Mock(return_value=query))
        with patch.object(handles, '_live_mutation_context', return_value=context), patch.object(handles, '_live_constructor_context', side_effect=AssertionError('unused global context')), patch.object(handles, '_group_target_context', side_effect=AssertionError('child context search')):
            result = value.get_anchors()
        self.assertEqual(float(result[0][0]), 1 + 2 ** -40)
        context.queryFamilyOwnPath.assert_called_once_with(family)
        family.pathQuery.assert_not_called()
        query.free.assert_called_once_with()
