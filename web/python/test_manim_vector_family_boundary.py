"""Thin vector-family observations preserve caller-owned Python hooks."""
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import noon
import _manim_compat as compat
import _manim_path_queries as queries
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class VectorFamilyBoundaryTests(unittest.TestCase):
    def family(self, member_type=compat.VMobject, empty=False):
        root = identity_only_wrapper(compat.VGroup)
        child = identity_only_wrapper(member_type)
        child._semantic_handle = SimpleNamespace(semanticSlot=1, semanticGeneration=0)
        child._semantic_handle_fresh = True
        root._semantic_family_handle = SimpleNamespace(
            semanticSlot=2, semanticGeneration=0,
            familyKeys=Mock(return_value=['2:0'] if empty else ['2:0', '1:0']),
            memberKeys=lambda: [] if empty else ['1:0'],
        )
        root._semantic_member_wrappers = {} if empty else {'1:0': child}
        return root, child

    def test_default_family_uses_one_shared_vector_snapshot(self):
        root, _ = self.family()
        with patch.object(queries, '_vector_boundary_points_observation', return_value=[0., 1., 3., 4.]) as observe, patch.object(queries, '_query', side_effect=AssertionError('per-member geometry query')):
            result = root.get_points_defining_boundary()
        self.assertEqual([list(p) for p in result], [[0., 1.], [3., 4.]])
        observe.assert_called_once_with(root)
        root._semantic_family_handle.familyKeys.assert_called_once_with()

    def test_empty_default_family_preserves_the_host_array_contract(self):
        root, _ = self.family(empty=True)
        with patch.object(queries, '_vector_boundary_points_observation', return_value=[]) as observe:
            result = root.get_points_defining_boundary()
        try:
            import numpy as np
        except ImportError:
            self.assertIsInstance(result, list)
            self.assertEqual(result, [])
        else:
            self.assertEqual(result.shape, (0,))
            self.assertEqual(result.dtype, np.dtype('float64'))
        observe.assert_called_once_with(root)

    def test_default_family_without_numpy_returns_vec2_values(self):
        root, _ = self.family()
        with patch.dict('sys.modules', {'numpy': None}), patch.object(queries, '_vector_boundary_points_observation', return_value=[1., 2., 3., 4.]):
            result = root.get_points_defining_boundary()
        self.assertEqual(result, [noon.Vec2(1, 2), noon.Vec2(3, 4)])

    def test_custom_family_selector_and_anchor_callable_are_preserved(self):
        root, child = self.family()
        root.get_family = Mock(return_value=[root, child])
        child.get_anchors = Mock(return_value=[noon.Vec2(9, 8)])
        with patch.object(queries, '_vector_boundary_points_observation', side_effect=AssertionError('custom family ignored')):
            result = root.get_points_defining_boundary()
        self.assertEqual([list(p) for p in result], [[9., 8.]])
        root.get_family.assert_called_once_with()
        child.get_anchors.assert_called_once_with()

    def test_anchor_descriptor_is_invoked_only_by_the_custom_hook_path(self):
        reads = []
        class Custom(compat.VMobject):
            @property
            def get_anchors(self):
                reads.append(True)
                return lambda: [noon.Vec2(5, 6)]
        root, _ = self.family(Custom)
        with patch.object(queries, '_vector_boundary_points_observation', side_effect=AssertionError('custom hook ignored')):
            result = root.get_points_defining_boundary()
        self.assertEqual([list(p) for p in result], [[5., 6.]])
        self.assertEqual(reads, [True])

    def test_custom_member_stride_uses_its_existing_anchor_adaptation(self):
        root, child = self.family()
        child.n_points_per_cubic_curve = 2
        query = Mock()
        query.points.return_value = [0., 0., 1., 0., 2., 0., 3., 0.]
        with patch.object(queries, '_typed_manim_observation', return_value=query), patch.object(queries, '_vector_boundary_points_observation', side_effect=AssertionError('custom stride ignored')):
            result = root.get_points_defining_boundary()
        self.assertEqual([list(p) for p in result], [[0., 0.], [1., 0.], [2., 0.], [3., 0.]])
        query.free.assert_called_once_with()

    def test_staged_family_query_rejects_before_reading_shared_members(self):
        root, _ = self.family()
        with patch('_manim_updaters.canonical_callback_phase_active', return_value=True), patch.object(queries, '_vector_boundary_points_observation') as observe:
            with self.assertRaisesRegex(NotImplementedError, 'staged callback'):
                root.get_points_defining_boundary()
        observe.assert_not_called()
        root._semantic_family_handle.familyKeys.assert_not_called()

    def test_typed_family_query_routes_to_its_current_owner_and_keeps_failure(self):
        root, _ = self.family()
        context = Mock()
        context.queryFamilyVectorBoundaryPoints.return_value = [1., 2.]
        with patch.object(handles, '_group_live_layout_context', return_value=context):
            self.assertEqual(handles._vector_boundary_points_observation(root), [1., 2.])
            context.queryFamilyVectorBoundaryPoints.side_effect = RuntimeError('stale publication')
            with self.assertRaisesRegex(Exception, 'stale publication'):
                handles._vector_boundary_points_observation(root)
        self.assertEqual(context.queryFamilyVectorBoundaryPoints.call_count, 2)
        context.queryFamilyVectorBoundaryPoints.assert_called_with(root._semantic_family_handle)
