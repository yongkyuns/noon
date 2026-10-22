"""Facade delegation; geometry correctness is tested against shared Rust bounds."""
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import noon
import _manim_compat

import _manim_semantic_handles as shared


class Observation:
    centerX, centerY, width, height = 7.0, -3.0, 10.0, 6.0

    def criticalX(self, x, y):
        return 2.0 if x < 0 else 12.0 if x > 0 else self.centerX

    def criticalY(self, x, y):
        return -6.0 if y < 0 else 0.0 if y > 0 else self.centerY

    def snapshotJson(self):
        raise AssertionError("layout must not request a geometry snapshot")


class ManimSharedLayoutQueryTests(unittest.TestCase):
    def test_staged_family_center_keeps_layout_guard_before_point_membership(self):
        from _typed_geometry_test_support import identity_only_wrapper
        import _manim_updaters
        value = identity_only_wrapper(noon.VGroup)
        point_read = Mock(side_effect=AssertionError("staged query read point membership"))
        value._semantic_family_handle = SimpleNamespace(allPoints3D=point_read, familyKeys=point_read)
        with patch.object(_manim_updaters, "canonical_callback_phase_active", return_value=True), patch.object(
            shared, "_group_layout_observation",
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
                    allPoints3D=point_query,
                    familyKeys=Mock(return_value=['2:0', '1:0']),
                    memberKeys=Mock(return_value=['1:0']))
                value._semantic_member_wrappers = {'1:0': text}
                with patch.object(shared, "_group_layout_observation", return_value=Observation()):
                    self.assertEqual(shared._get_center(value), (7., -3.))
                    self.assertEqual(shared._get_critical_point(value, noon.RIGHT), (12., -3.))
                point_query.assert_not_called()
                self.assertEqual(value._semantic_family_handle.familyKeys.call_count, 2)

    def test_custom_family_query_is_not_replaced_by_content_layout_routing(self):
        from _typed_geometry_test_support import identity_only_wrapper
        value = identity_only_wrapper(noon.VGroup)
        value._semantic_family_handle = SimpleNamespace(
            allPoints3D=Mock(), familyKeys=Mock(side_effect=AssertionError("custom selector")))
        value.get_family = Mock(return_value=[])
        self.assertTrue(shared._point_boundary_queries_available(value))
        value.get_family.assert_not_called()
        value._semantic_family_handle.familyKeys.assert_not_called()

    def test_detached_layout_delegates_without_materializing_geometry(self):
        value = self.wrapper(Observation())
        self.assertEqual(shared._get_center(value), (7, -3))
        self.assertEqual(shared._width(value), 10)
        self.assertEqual(shared._height(value), 6)
        self.assertEqual(shared._layout_bounds(value), ((2, -6), (12, 0)))
        self.assertEqual(shared._get_critical_point(value, noon.RIGHT), (12, -3))

    def test_bound_layout_uses_the_owning_live_context(self):
        value = self.wrapper(object())
        observed = Observation()
        calls = []
        def query(handle):
            calls.append(handle)
            return observed
        dimension = Mock(return_value=10.0)
        value._scene = SimpleNamespace(_canonical_authoring_context=SimpleNamespace(
            queryMobjectLayout=query, queryMobjectDimension=dimension))
        value._object = object()
        self.assertEqual(shared._get_center(value), (7, -3))
        self.assertEqual(shared._width(value), 10)
        self.assertEqual(shared._get_critical_point(value, noon.UP), (7, 0))
        self.assertEqual(calls, [value._semantic_handle] * 2)
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
