"""Facade delegation; geometry correctness is tested against shared Rust bounds."""
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import noon
import _manim_compat

import _manim_semantic_handles as shared


class Observation:
    centerX, centerY, width, height = 7.0, -3.0, 10.0, 6.0

    def requiresContentBoundary(self):
        return True

    def criticalX(self, x, y):
        return 2.0 if x < 0 else 12.0 if x > 0 else self.centerX

    def criticalY(self, x, y):
        return -6.0 if y < 0 else 0.0 if y > 0 else self.centerY

    def snapshotJson(self):
        raise AssertionError("layout must not request a geometry snapshot")

    def layoutAnchor(self, index):
        return self

    def contentCriticalPoint3D(self, x, y, z):
        return self.criticalX(x, y), self.criticalY(x, y), 1.125

    def free(self):
        pass


class ManimSharedLayoutQueryTests(unittest.TestCase):
    def assert_point_result(self, result, expected):
        self.assertEqual(list(result), expected)
        try:
            import numpy as np
        except ModuleNotFoundError as error:
            if error.name != "numpy":
                raise
            self.assertIsInstance(result, list)
        else:
            self.assertIsInstance(result, np.ndarray)
            self.assertEqual(result.shape, (3,))
            self.assertEqual(result.dtype, np.dtype("float64"))

    def test_public_retained_center_and_planar_edges_keep_rust_z_and_array_shape(self):
        from _typed_geometry_test_support import identity_only_wrapper
        from _manim_typst import Text
        value = identity_only_wrapper(Text)
        query = Mock(return_value=(7., -3., 2.375))
        anchor = SimpleNamespace(contentCriticalPoint3D=query, free=Mock())
        with patch.object(shared, "_layout_anchor", return_value=anchor), \
             patch.object(shared, "_live_mutation_context", return_value=None):
            self.assert_point_result(value.get_center(), [7., -3., 2.375])
            self.assert_point_result(value.get_left(), [7., -3., 2.375])
            self.assert_point_result(value.get_right(), [7., -3., 2.375])
            self.assert_point_result(value.get_top(), [7., -3., 2.375])
            self.assert_point_result(value.get_bottom(), [7., -3., 2.375])
        self.assertEqual(query.call_args_list, [
            unittest.mock.call(0., 0., 0.), unittest.mock.call(-1., 0., 0.),
            unittest.mock.call(1., 0., 0.), unittest.mock.call(0., 1., 0.),
            unittest.mock.call(0., -1., 0.)])
        self.assertEqual(anchor.free.call_count, 5)

    def test_bound_content_query_keeps_live_values_from_owning_context(self):
        anchor = SimpleNamespace(contentCriticalPoint3D=Mock(side_effect=AssertionError("authored value")), free=Mock())
        value = self.wrapper(SimpleNamespace(requiresContentBoundary=Mock(), layoutAnchor=Mock(return_value=anchor)))
        query = Mock(return_value=(1.25, -4.5, -3.5))
        context = SimpleNamespace(requiresLiveMutationRouting=Mock(return_value=True),
                                  queryMobjectRequiresContentBoundary=Mock(return_value=True),
                                  queryLayoutContentCriticalPoint3D=query)
        value._scene = SimpleNamespace(_canonical_authoring_context=context)
        value._object = object()
        self.assert_point_result(value.get_center(), [1.25, -4.5, -3.5])
        query.assert_called_once_with(anchor, 0., 0., 0.)
        anchor.contentCriticalPoint3D.assert_not_called()
        anchor.free.assert_called_once_with()

    def test_bound_content_before_first_play_queries_authored_anchor_in_owning_scene(self):
        anchor = SimpleNamespace(contentCriticalPoint3D=Mock(return_value=(1.25, -4.5, 2.375)),
                                 free=Mock())
        value = self.wrapper(SimpleNamespace(requiresContentBoundary=Mock(return_value=True),
                                             layoutAnchor=Mock(return_value=anchor)))
        context = SimpleNamespace(requiresLiveMutationRouting=Mock(return_value=False),
                                  queryMobjectRequiresContentBoundary=Mock(return_value=True),
                                  queryLayoutContentCriticalPoint3D=Mock(
                                      side_effect=AssertionError("begin live execution before querying")))
        foreign_context = SimpleNamespace(requiresLiveMutationRouting=Mock(
            side_effect=AssertionError("use owning scene, not old constructor context")))
        value._canonical_live_target_context = foreign_context
        value._scene = SimpleNamespace(_canonical_authoring_context=context)
        value._object = object()
        self.assert_point_result(value.get_center(), [1.25, -4.5, 2.375])
        context.requiresLiveMutationRouting.assert_called_once_with()
        context.queryLayoutContentCriticalPoint3D.assert_not_called()
        foreign_context.requiresLiveMutationRouting.assert_not_called()
        anchor.contentCriticalPoint3D.assert_called_once_with(0., 0., 0.)
        anchor.free.assert_called_once_with()

    def test_content_query_failure_frees_anchor_and_does_not_read_planar_bounds(self):
        error = RuntimeError("execution is transferred")
        anchor = SimpleNamespace(contentCriticalPoint3D=Mock(side_effect=error), free=Mock())
        value = self.wrapper(SimpleNamespace(requiresContentBoundary=Mock(return_value=True),
                                             layoutAnchor=Mock(return_value=anchor)))
        with patch.object(shared, "_layout_center", side_effect=AssertionError("planar fallback")):
            with self.assertRaises(RuntimeError) as raised:
                value.get_center()
        self.assertIs(raised.exception, error)
        anchor.free.assert_called_once_with()

    def test_family_context_guard_precedes_anchor_creation(self):
        from _typed_geometry_test_support import identity_only_wrapper
        value = identity_only_wrapper(noon.VGroup)
        create = Mock(side_effect=AssertionError("anchor leaked before routing guard"))
        value._semantic_family_handle = SimpleNamespace(requiresContentBoundary=Mock(return_value=True),
                                                        layoutAnchor=create)
        with patch.object(shared, "_group_live_layout_context", side_effect=NotImplementedError("active callback phase")):
            with self.assertRaisesRegex(NotImplementedError, "active callback phase"):
                shared._content_critical_point_3d(value, noon.RIGHT)
        create.assert_not_called()

    def test_generic_retained_leaf_selects_authored_content_layout_in_rust(self):
        anchor = SimpleNamespace(contentCriticalPoint3D=Mock(return_value=(7., -3., 1.125)), free=Mock())
        handle = SimpleNamespace(
            requiresContentBoundary=Mock(return_value=True), layoutAnchor=Mock(return_value=anchor),
            criticalPoint3D=Mock(side_effect=AssertionError("retained content is not a path")))
        value = self.wrapper(handle)
        with patch.object(shared, "_layout_center", side_effect=AssertionError("stale planar result")):
            self.assertEqual(list(shared._get_center(value)), [7., -3., 1.125])
        handle.requiresContentBoundary.assert_called_once_with()
        handle.criticalPoint3D.assert_not_called()
        handle.layoutAnchor.assert_called_once_with(None)
        anchor.contentCriticalPoint3D.assert_called_once_with(0., 0., 0.)
        anchor.free.assert_called_once_with()

    def test_generic_bound_retained_leaf_uses_owning_context_classification(self):
        anchor = SimpleNamespace(contentCriticalPoint3D=Mock(side_effect=AssertionError("authored read")), free=Mock())
        handle = SimpleNamespace(requiresContentBoundary=Mock(side_effect=AssertionError("stale authored query")),
                                 layoutAnchor=Mock(return_value=anchor))
        value = self.wrapper(handle)
        context = SimpleNamespace(
            requiresLiveMutationRouting=Mock(return_value=True),
            queryMobjectRequiresContentBoundary=Mock(return_value=True),
            queryLayoutContentCriticalPoint3D=Mock(return_value=(7., -3., -3.5)),
            queryMobjectLayout=Mock(side_effect=AssertionError("stale planar result")))
        value._scene = SimpleNamespace(_canonical_authoring_context=context)
        value._object = object()
        self.assertEqual(list(shared._get_center(value)), [7., -3., -3.5])
        context.queryMobjectRequiresContentBoundary.assert_called_once_with(handle)
        context.queryLayoutContentCriticalPoint3D.assert_called_once_with(anchor, 0., 0., 0.)
        context.queryMobjectLayout.assert_not_called()
        handle.requiresContentBoundary.assert_not_called()
        anchor.contentCriticalPoint3D.assert_not_called()
        anchor.free.assert_called_once_with()

    def test_generic_geometry_keeps_point_boundary_behavior_without_wrapper_traversal(self):
        handle = SimpleNamespace(requiresContentBoundary=Mock(return_value=False))
        value = self.wrapper(handle)
        with patch("_manim_path_queries.critical_point", return_value=(1., 2., 3.)) as point:
            self.assertEqual(shared._get_center(value), (1., 2., 3.))
        point.assert_called_once_with(value, (0., 0., 0.))
        handle.requiresContentBoundary.assert_called_once_with()

    def test_live_family_classification_does_not_walk_python_members(self):
        from _typed_geometry_test_support import identity_only_wrapper
        value = identity_only_wrapper(noon.Group)
        anchor = SimpleNamespace(contentCriticalPoint3D=Mock(side_effect=AssertionError("authored read")), free=Mock())
        handle = SimpleNamespace(requiresContentBoundary=Mock(side_effect=AssertionError("authored read")),
                                 layoutAnchor=Mock(return_value=anchor))
        value._semantic_family_handle = handle
        context = SimpleNamespace(queryFamilyRequiresContentBoundary=Mock(return_value=True),
                                  queryLayoutContentCriticalPoint3D=Mock(return_value=(7., -3., -1.1875)))
        with patch.object(shared, "_group_live_layout_context", return_value=context), \
             patch.object(shared, "_group_layout_observation", side_effect=AssertionError("planar query")), \
             patch.object(_manim_compat, "_leaf_mobjects", side_effect=AssertionError("wrapper traversal")):
            self.assertEqual(list(shared._get_center(value)), [7., -3., -1.1875])
        context.queryFamilyRequiresContentBoundary.assert_called_once_with(handle)
        context.queryLayoutContentCriticalPoint3D.assert_called_once_with(anchor, 0., 0., 0.)
        handle.requiresContentBoundary.assert_not_called()
        anchor.contentCriticalPoint3D.assert_not_called()
        anchor.free.assert_called_once_with()

    def test_custom_query_hooks_precede_native_content_selection(self):
        handle = SimpleNamespace(requiresContentBoundary=Mock(side_effect=AssertionError("must preserve caller hook")))
        value = self.wrapper(handle)
        value.get_all_points = Mock(return_value=[])
        with patch("_manim_path_queries.critical_point", return_value=(0., 0., 0.)) as point:
            self.assertEqual(shared._get_center(value), (0., 0., 0.))
        point.assert_called_once_with(value, (0., 0., 0.))
        handle.requiresContentBoundary.assert_not_called()

    def test_native_profile_failure_is_not_retried_with_a_different_query(self):
        error = RuntimeError("execution is transferred")
        handle = SimpleNamespace(requiresContentBoundary=Mock(side_effect=error))
        value = self.wrapper(handle)
        with patch.object(shared, "_layout_center") as layout:
            with self.assertRaises(RuntimeError) as raised:
                shared._get_center(value)
        self.assertIs(raised.exception, error)
        layout.assert_not_called()

    def test_staged_bound_object_does_not_add_an_ordinary_content_query(self):
        value = self.wrapper(object())
        context = SimpleNamespace(queryMobjectRequiresContentBoundary=Mock())
        value._scene = SimpleNamespace(_canonical_authoring_context=context)
        value._object = object()
        with patch.object(shared, "_handle_for", return_value=None), \
             patch("_manim_path_queries.critical_point", return_value=(1., 2., 3.)) as point:
            self.assertEqual(shared._get_center(value), (1., 2., 3.))
        point.assert_called_once_with(value, (0., 0., 0.))
        context.queryMobjectRequiresContentBoundary.assert_not_called()

    def test_staged_family_center_keeps_layout_guard_before_point_membership(self):
        from _typed_geometry_test_support import identity_only_wrapper
        import _manim_updaters
        value = identity_only_wrapper(noon.VGroup)
        point_read = Mock(side_effect=AssertionError("staged query read point membership"))
        value._semantic_family_handle = SimpleNamespace(requiresContentBoundary=point_read, familyKeys=point_read)
        with patch.object(_manim_updaters, "canonical_callback_phase_active", return_value=True), patch.object(
            shared, "_group_live_layout_context",
            side_effect=NotImplementedError("family layout query during active callback phase"),
        ):
            with self.assertRaisesRegex(NotImplementedError, "active callback phase"):
                value.get_center()
        point_read.assert_not_called()

    def test_content_depth_queries_forward_to_authored_and_live_rust_layout(self):
        from _typed_geometry_test_support import identity_only_wrapper
        from _manim_typst import Text
        value = identity_only_wrapper(Text)
        anchor = SimpleNamespace(contentCriticalPoint3D=Mock(return_value=(7., -3., 1.125)), free=Mock())
        context = SimpleNamespace(queryLayoutContentCriticalPoint3D=Mock(return_value=(7., -3., -2.5)))
        with patch.object(shared, "_layout_anchor", return_value=anchor), patch.object(
            shared, "_live_mutation_context", return_value=None
        ):
            self.assertEqual(list(value.get_zenith()), [7., -3., 1.125])
        anchor.contentCriticalPoint3D.assert_called_once_with(0., 0., 1.)
        with patch.object(shared, "_layout_anchor", return_value=anchor), patch.object(
            shared, "_live_mutation_context", return_value=context
        ):
            self.assertEqual(list(value.get_nadir()), [7., -3., -2.5])
        context.queryLayoutContentCriticalPoint3D.assert_called_once_with(anchor, 0., 0., -1.)
        self.assertEqual(anchor.free.call_count, 2)

    def wrapper(self, handle):
        value = object.__new__(noon.Mobject)
        shared._attach_shared_handle(value, handle)
        return value

    def test_mixed_text_families_keep_shared_content_bounds(self):
        from _typed_geometry_test_support import identity_only_wrapper
        from _manim_typst import Text
        for kind in (noon.Group, noon.VGroup):
            with self.subTest(kind=kind.__name__):
                value = identity_only_wrapper(kind)
                text = identity_only_wrapper(Text)
                text._semantic_handle = SimpleNamespace(semanticSlot=1, semanticGeneration=0)
                text._semantic_handle_fresh = True
                point_query = Mock(side_effect=AssertionError("opaque content is not a point array"))
                value._semantic_family_handle = SimpleNamespace(
                    semanticSlot=2, semanticGeneration=0,
                    requiresContentBoundary=Mock(return_value=True),
                    allPoints3D=point_query,
                    familyKeys=Mock(return_value=['2:0', '1:0']),
                    memberKeys=Mock(return_value=['1:0']), layoutAnchor=Mock(return_value=Observation()))
                value._semantic_member_wrappers = {'1:0': text}
                with patch.object(shared, "_group_layout_observation", return_value=Observation()):
                    self.assertEqual(list(shared._get_center(value)), [7., -3., 1.125])
                    self.assertEqual(list(shared._get_critical_point(value, noon.RIGHT)), [12., -3., 1.125])
                point_query.assert_not_called()
                self.assertEqual(value._semantic_family_handle.requiresContentBoundary.call_count, 2)
                value._semantic_family_handle.familyKeys.assert_not_called()

    def test_custom_family_query_is_not_replaced_by_content_layout_routing(self):
        from _typed_geometry_test_support import identity_only_wrapper
        value = identity_only_wrapper(noon.VGroup)
        value._semantic_family_handle = SimpleNamespace(
            requiresContentBoundary=Mock(side_effect=AssertionError("custom selector")),
            allPoints3D=Mock(), familyKeys=Mock(side_effect=AssertionError("custom selector")))
        value.get_family = Mock(return_value=[])
        self.assertTrue(shared._point_boundary_queries_available(value))
        value.get_family.assert_not_called()
        value._semantic_family_handle.familyKeys.assert_not_called()

    def test_detached_layout_delegates_without_materializing_geometry(self):
        value = self.wrapper(Observation())
        self.assertEqual(list(shared._get_center(value)), [7., -3., 1.125])
        self.assertEqual(shared._width(value), 10)
        self.assertEqual(shared._height(value), 6)
        self.assertEqual(shared._layout_bounds(value), ((2, -6), (12, 0)))
        self.assertEqual(list(shared._get_critical_point(value, noon.RIGHT)), [12., -3., 1.125])

    def test_bound_layout_uses_the_owning_live_context(self):
        anchor = Observation()
        value = self.wrapper(SimpleNamespace(layoutAnchor=Mock(return_value=anchor)))
        query = Mock(side_effect=AssertionError("planar result"))
        content = Mock(side_effect=lambda source, x, y, z: (anchor.criticalX(x, y), anchor.criticalY(x, y), -3.5))
        dimension = Mock(return_value=10.0)
        value._scene = SimpleNamespace(_canonical_authoring_context=SimpleNamespace(
            requiresLiveMutationRouting=Mock(return_value=True),
            queryMobjectRequiresContentBoundary=Mock(return_value=True),
            queryMobjectLayout=query, queryMobjectDimension=dimension, queryLayoutContentCriticalPoint3D=content))
        value._object = object()
        self.assertEqual(list(shared._get_center(value)), [7., -3., -3.5])
        self.assertEqual(shared._width(value), 10)
        self.assertEqual(list(shared._get_critical_point(value, noon.UP)), [7., 0., -3.5])
        self.assertEqual(content.call_count, 2)
        query.assert_not_called()
        dimension.assert_called_once_with(value._semantic_handle, 0)

    def test_dimensions_do_not_request_a_rejected_anchor_observation(self):
        value = self.wrapper(object())
        context = SimpleNamespace(
            queryMobjectLayout=Mock(side_effect=ValueError("unpaired anchors")),
            queryMobjectDimension=Mock(side_effect=lambda handle, dim: (3.0, 7.0)[dim]),
        )
        value._scene = SimpleNamespace(_canonical_authoring_context=context)
        value._object = object()
        self.assertEqual((shared._width(value), shared._height(value)), (3.0, 7.0))
        context.queryMobjectLayout.assert_not_called()
        context.queryMobjectDimension.side_effect = RuntimeError("execution is transferred")
        with self.assertRaisesRegex(RuntimeError, "transferred"):
            shared._width(value)

    def test_family_dimensions_use_the_shared_dimension_only_query(self):
        value = object.__new__(_manim_compat.VGroup)
        value._semantic_family_handle = object()
        anchor = SimpleNamespace(dimensionLength=Mock(side_effect=lambda dim: (3.0, 7.0)[dim]))
        context = SimpleNamespace(queryLayoutDimensionLength=Mock(side_effect=lambda handle, dim: (6.0, 14.0)[dim]))
        with patch.object(shared, "_layout_anchor", return_value=anchor), patch.object(
            shared, "_group_live_layout_context", return_value=None
        ), patch.object(shared, "_group_layout_observation", side_effect=ValueError("unpaired anchors")):
            self.assertEqual((shared._width(value), shared._height(value)), (3.0, 7.0))
        with patch.object(shared, "_layout_anchor", return_value=anchor), patch.object(
            shared, "_group_live_layout_context", return_value=context
        ):
            self.assertEqual((shared._width(value), shared._height(value)), (6.0, 14.0))

    def test_missing_shared_layout_never_falls_back_to_snapshot_math(self):
        for handle in (None, SimpleNamespace(snapshotJson=lambda: self.fail("raw fallback"))):
            value = self.wrapper(handle)
            for query in (shared._get_center, shared._width, shared._height, shared._layout_bounds):
                with self.subTest(handle=handle, query=query.__name__):
                    with self.assertRaises((RuntimeError, AttributeError)):
                        query(value)

    def test_copy_and_target_capture_require_a_shared_handle(self):
        value = self.wrapper(None)
        value._current_raw = lambda: self.fail("copy must not materialize raw state")
        for target_state in (False, True):
            with self.assertRaisesRegex(RuntimeError, "shared Rust"):
                shared._clone_mobject(value, target_state=target_state)
        with self.assertRaisesRegex(NotImplementedError, "raw replacement"):
            shared._apply(value, object())


if __name__ == "__main__":
    unittest.main()
